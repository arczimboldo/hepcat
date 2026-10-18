//! Effects for microphones, apps and buses. A [`Processor`] runs the chain
//! on 10 ms frames of mono (microphones) or stereo (apps and buses) audio;
//! [`FxNode`] wraps it in PipeWire streams and puts it in the signal path.
//!
//! Chain: gain → WebRTC (echo cancellation, steady-noise suppression,
//! automatic gain) → RNNoise voice isolation → gate → EQ → compressor →
//! pitch → voice effect → limiter. Gate, compressor and limiter react to the
//! loudest channel, so a stereo image does not shift.

mod apm;
pub mod dsp;
mod node;

pub use node::{FX_IN_PREFIX, FX_OUT_PREFIX, FX_PREFIX, FX_REF_PREFIX, FxNames, FxNode};

use crate::config::{Effects, NoiseMode, VoiceEffect};
use apm::{Apm, ApmConfig};
use dsp::{Biquad, Compressor, Gate, Limiter, PitchShifter, Smoothed, VoiceFx, db_to_gain};
use nnnoiseless::DenoiseState;
use std::collections::VecDeque;

pub const RATE: u32 = 48_000;
/// 10 ms, the block size of both WebRTC and RNNoise.
pub const FRAME: usize = (RATE / 100) as usize;
pub const MAX_CHANNELS: usize = 2;
/// Echo reference older than this is useless to the canceller.
const MAX_REFERENCE: usize = FRAME * 10;

/// Voice isolation at the strongest setting mutes the signal once RNNoise
/// has heard no speech for this many frames.
const VOICE_HOLD: u32 = 25;

/// Per-channel state of the blocks that work on each channel separately.
#[derive(Default)]
struct Lane {
    denoise: Option<Box<DenoiseState<'static>>>,
    /// The previous input frame: RNNoise's output lags its input by one frame,
    /// so this is the dry signal that lines up with it.
    dry: Option<Box<[f32; FRAME]>>,
    eq: [Biquad; 3],
    pitch: PitchShifter,
    voice: VoiceFx,
}

pub struct Processor {
    fx: Effects,
    channels: usize,
    /// Interleaved input waiting for a full frame.
    input: Vec<f32>,
    /// Interleaved processed audio waiting to be played.
    output: VecDeque<f32>,
    reference: VecDeque<f32>,
    /// Largest block (in frames) a reader pulled; bounds how much output may queue up.
    pull_size: usize,
    apm: Option<(Apm, ApmConfig)>,
    gain: Smoothed,
    gate: Gate,
    compressor: Compressor,
    limiter: Limiter,
    eq_settings: [f32; 3],
    lanes: Vec<Lane>,
    /// Gain and hold counter of the speech gate used by maximum voice isolation.
    voice_gate: f32,
    voice_hold: u32,
    peak: f32,
}

impl Processor {
    /// `channels` is 1 or 2.
    pub fn new(fx: &Effects, channels: usize) -> Self {
        let channels = channels.clamp(1, MAX_CHANNELS);
        let mut p = Self {
            fx: *fx,
            channels,
            input: Vec::with_capacity(FRAME * channels),
            output: VecDeque::new(),
            reference: VecDeque::new(),
            pull_size: FRAME,
            apm: None,
            gain: Smoothed::new(db_to_gain(fx.gain_db)),
            gate: Gate::default(),
            compressor: Compressor::default(),
            limiter: Limiter::default(),
            eq_settings: [0.0; 3],
            lanes: (0..channels).map(|_| Lane::default()).collect(),
            voice_gate: 1.0,
            voice_hold: VOICE_HOLD,
            peak: 0.0,
        };
        p.set_effects(fx);
        p
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn set_effects(&mut self, fx: &Effects) {
        self.fx = *fx;
        self.gain.target = db_to_gain(fx.gain_db);

        let eq = [fx.bass_db, fx.presence_db, fx.treble_db];
        if eq != self.eq_settings {
            for lane in &mut self.lanes {
                lane.eq[0].retune(Biquad::low_shelf(180.0, eq[0]));
                lane.eq[1].retune(Biquad::peaking(3000.0, 0.9, eq[1]));
                lane.eq[2].retune(Biquad::high_shelf(8000.0, eq[2]));
            }
            self.eq_settings = eq;
        }

        let wanted = (fx.echo || fx.noise == NoiseMode::Standard || fx.auto_gain).then_some(ApmConfig {
            echo: fx.echo,
            // WebRTC's levels run from 0 (low, about 6 dB) to 3 (very high, about 21 dB).
            noise: (fx.noise == NoiseMode::Standard).then_some(fx.noise_strength.clamp(1, 4) - 1),
            auto_gain: fx.auto_gain,
        });
        match (wanted, &mut self.apm) {
            (None, _) => self.apm = None,
            (Some(cfg), Some((apm, current))) => {
                if *current != cfg {
                    apm.configure(cfg);
                    *current = cfg;
                }
            }
            (Some(cfg), None) => match Apm::new(RATE, self.channels) {
                Some(mut apm) => {
                    apm.configure(cfg);
                    self.apm = Some((apm, cfg));
                }
                None => eprintln!("hepcat: could not start WebRTC audio processing"),
            },
        }
        if !fx.echo {
            self.reference.clear();
        }

        let voice = fx.noise == NoiseMode::Voice;
        for lane in &mut self.lanes {
            match (voice, lane.denoise.is_some()) {
                (true, false) => lane.denoise = Some(DenoiseState::new()),
                (false, true) => lane.denoise = None,
                _ => {}
            }
        }
    }

    pub fn wants_reference(&self) -> bool {
        self.fx.echo
    }

    /// Interleaved samples in; processed frames go to the output queue.
    pub fn push_input(&mut self, samples: impl IntoIterator<Item = f32>) {
        let ch = self.channels;
        for s in samples {
            self.input.push(s);
            if self.input.len() == FRAME * ch {
                let mut planes = [[0.0; FRAME]; MAX_CHANNELS];
                for (i, s) in self.input.drain(..).enumerate() {
                    planes[i % ch][i / ch] = s;
                }
                self.process_frame(&mut planes[..ch]);
                for i in 0..FRAME {
                    self.output.extend(planes[..ch].iter().map(|p| p[i]));
                }
            }
        }
        // Nobody is listening (or they fell behind): drop the oldest audio
        // instead of building up latency.
        let limit = (self.pull_size * 3 + FRAME * 2) * ch;
        if self.output.len() > limit {
            let excess = self.output.len() - (self.pull_size + FRAME) * ch;
            self.output.drain(..excess);
        }
    }

    /// What the speakers play (mono), for the echo canceller.
    pub fn push_reference(&mut self, samples: impl IntoIterator<Item = f32>) {
        if !self.fx.echo {
            return;
        }
        self.reference.extend(samples);
        if self.reference.len() > MAX_REFERENCE {
            let excess = self.reference.len() - MAX_REFERENCE / 2;
            self.reference.drain(..excess);
        }
    }

    /// Fill `out` with interleaved processed audio, padding with silence on underrun.
    #[cfg(test)]
    pub fn pull(&mut self, out: &mut [f32]) {
        self.pull_size = self.pull_size.max(out.len() / self.channels);
        for s in out {
            *s = self.output.pop_front().unwrap_or(0.0);
        }
    }

    /// Write `frames` frames of processed audio into a buffer as 32-bit
    /// little-endian floats, padding with silence on underrun. Does not
    /// allocate, so it is safe on the realtime thread.
    pub fn pull_into(&mut self, bytes: &mut [u8], frames: usize) {
        self.pull_size = self.pull_size.max(frames);
        for out in bytes.chunks_exact_mut(4).take(frames * self.channels) {
            out.copy_from_slice(&self.output.pop_front().unwrap_or(0.0).to_le_bytes());
        }
    }

    /// Peak output level since the last call.
    pub fn take_peak(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }

    /// RNNoise on every channel. Below full strength some of the dry signal
    /// is mixed back in, which caps how far noise is pushed down; at maximum,
    /// everything is muted while no channel holds speech.
    fn isolate_voice(&mut self, planes: &mut [[f32; FRAME]]) {
        let strength = self.fx.noise_strength.clamp(1, 4);
        // How much dry signal stays: -10 dB (light), -20 dB (medium), none.
        let dry_gain = match strength {
            1 => 0.316,
            2 => 0.1,
            _ => 0.0,
        };
        let mut vad = 0.0f32;
        for (plane, lane) in planes.iter_mut().zip(&mut self.lanes) {
            let Some(denoise) = &mut lane.denoise else { continue };
            // RNNoise expects 16-bit sample values.
            let input = plane.map(|s| s * 32768.0);
            let mut out = [0.0; FRAME];
            vad = vad.max(denoise.process_frame(&mut out, &input));
            let dry = lane.dry.get_or_insert_with(|| Box::new([0.0; FRAME]));
            for ((s, wet), d) in plane.iter_mut().zip(out).zip(dry.iter()) {
                *s = wet / 32768.0 + d * dry_gain;
            }
            dry.copy_from_slice(&input.map(|s| s / 32768.0));
        }
        if strength < 4 {
            self.voice_gate = 1.0;
            return;
        }
        self.voice_hold = if vad >= 0.5 { VOICE_HOLD } else { self.voice_hold.saturating_sub(1) };
        let target = if self.voice_hold > 0 { 1.0 } else { 0.0 };
        // Fade over the frame so the gate never clicks.
        let start = self.voice_gate;
        for (i, k) in (1..=FRAME).map(|i| (i, i as f32 / FRAME as f32)) {
            let g = start + (target - start) * k;
            planes.iter_mut().for_each(|p| p[i - 1] *= g);
        }
        self.voice_gate = target;
    }

    fn process_frame(&mut self, planes: &mut [[f32; FRAME]]) {
        let fx = self.fx;
        for i in 0..FRAME {
            let g = self.gain.next();
            for plane in planes.iter_mut() {
                plane[i] *= g;
            }
        }

        if let Some((apm, cfg)) = &mut self.apm {
            if cfg.echo {
                let mut reference = [0.0; FRAME];
                if self.reference.len() >= FRAME {
                    for (r, s) in reference.iter_mut().zip(self.reference.drain(..FRAME)) {
                        *r = s;
                    }
                }
                apm.render(&mut reference);
            }
            apm.capture(planes);
        }

        if fx.noise == NoiseMode::Voice {
            self.isolate_voice(planes);
        }

        let eq_on = self.eq_settings.iter().any(|&db| db != 0.0);
        let pitch_on = fx.pitch.abs() > 0.05;
        let peak = |planes: &[[f32; FRAME]], i: usize| planes.iter().fold(0.0f32, |m, p| m.max(p[i].abs()));
        for i in 0..FRAME {
            if fx.gate {
                let g = self.gate.gain(peak(planes, i), fx.gate_db);
                planes.iter_mut().for_each(|p| p[i] *= g);
            }
            for (plane, lane) in planes.iter_mut().zip(&mut self.lanes) {
                if eq_on {
                    plane[i] = lane.eq.iter_mut().fold(plane[i], |x, f| f.process(x));
                }
            }
            if fx.compressor > 0.0 {
                let g = self.compressor.gain(peak(planes, i), fx.compressor);
                planes.iter_mut().for_each(|p| p[i] *= g);
            }
            for (plane, lane) in planes.iter_mut().zip(&mut self.lanes) {
                let mut x = plane[i];
                if pitch_on {
                    x = lane.pitch.process(x, fx.pitch);
                }
                if fx.voice != VoiceEffect::None {
                    let wet = lane.voice.process(x, fx.voice);
                    x += (wet - x) * fx.voice_mix;
                }
                plane[i] = x;
            }
            let g = self.limiter.gain(peak(planes, i));
            for plane in planes.iter_mut() {
                let x = (plane[i] * g).clamp(-1.0, 1.0);
                plane[i] = if x.is_finite() { x } else { 0.0 };
                self.peak = self.peak.max(plane[i].abs());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn run(fx: &Effects, input: &[f32]) -> Vec<f32> {
        let mut p = Processor::new(fx, 1);
        let mut out = vec![0.0; input.len()];
        for (i, o) in input.chunks(FRAME).zip(out.chunks_mut(FRAME)) {
            p.push_input(i.iter().copied());
            p.pull(o);
        }
        out
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Deterministic white noise.
    fn noise(len: usize, amp: f32) -> Vec<f32> {
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    #[test]
    fn disabled_chain_passes_audio_through() {
        let input: Vec<f32> = (0..RATE as usize).map(|i| 0.3 * (TAU * 440.0 * i as f32 / RATE as f32).sin()).collect();
        let out = run(&Effects::default(), &input);
        assert!((rms(&out) - rms(&input)).abs() < 0.01);
    }

    #[test]
    fn gain_is_applied() {
        let input = vec![0.1; RATE as usize];
        let fx = Effects { gain_db: 6.0, ..Effects::default() };
        let out = run(&fx, &input);
        assert!((out[RATE as usize - 1] - 0.1995).abs() < 0.005);
    }

    #[test]
    fn standard_suppression_removes_steady_noise_but_keeps_a_clap() {
        // Three seconds of hiss with a loud burst (a "clap") near the end.
        let len = RATE as usize * 3;
        let mut input = noise(len, 0.02);
        let clap = len - RATE as usize / 2;
        for (s, n) in input[clap..clap + 2400].iter_mut().zip(noise(2400, 0.8)) {
            *s += n;
        }
        let fx = Effects { noise: NoiseMode::Standard, ..Effects::default() };
        let out = run(&fx, &input);
        let hiss = rms(&out[clap - RATE as usize..clap - FRAME * 4]);
        assert!(hiss < rms(&input[..RATE as usize]) * 0.5, "hiss not reduced: {hiss}");
        let burst = rms(&out[clap..clap + 2400 + FRAME * 2]);
        assert!(burst > 0.2, "clap was suppressed: {burst}");
    }

    #[test]
    fn voice_isolation_removes_noise() {
        // Low-passed noise sounds like a fan. (RNNoise mistakes perfectly white
        // noise for speech, which never happens with a real microphone.)
        let mut lp = 0.0;
        let input: Vec<f32> = noise(RATE as usize * 2, 0.1)
            .into_iter()
            .map(|x| {
                lp = lp * 0.9 + x * 0.1;
                lp * 3.0
            })
            .collect();
        let fx = Effects { noise: NoiseMode::Voice, ..Effects::default() };
        let out = run(&fx, &input);
        let ratio = rms(&out[RATE as usize..]) / rms(&input);
        assert!(ratio < 0.2, "noise left: {ratio}");
    }

    #[test]
    fn noise_strength_sets_how_much_noise_is_left() {
        let mut lp = 0.0;
        let input: Vec<f32> = noise(RATE as usize * 3, 0.1)
            .into_iter()
            .map(|x| {
                lp = lp * 0.9 + x * 0.1;
                lp * 3.0
            })
            .collect();
        let left = |noise, noise_strength| {
            let out = run(&Effects { noise, noise_strength, ..Effects::default() }, &input);
            rms(&out[RATE as usize * 2..]) / rms(&input)
        };
        for mode in [NoiseMode::Standard, NoiseMode::Voice] {
            let levels: Vec<f32> = (1..=4).map(|s| left(mode, s)).collect();
            assert!(levels.windows(2).all(|w| w[0] >= w[1]), "{mode:?}: {levels:?}");
            assert!(levels[0] > levels[3] * 1.5, "{mode:?}: {levels:?}");
        }
        // Maximum voice isolation mutes whatever RNNoise hears no speech in,
        // such as mains hum, entirely.
        let hum: Vec<f32> = (0..RATE as usize * 2)
            .map(|i| (1..6).map(|h| 0.05 / h as f32 * (TAU * 50.0 * h as f32 * i as f32 / RATE as f32).sin()).sum())
            .collect();
        let fx = |noise_strength| Effects { noise: NoiseMode::Voice, noise_strength, ..Effects::default() };
        let strong = rms(&run(&fx(3), &hum)[RATE as usize..]);
        let max = rms(&run(&fx(4), &hum)[RATE as usize..]);
        assert!(max < 1e-6 && strong > 1e-4, "hum left: {max} (strong: {strong})");
    }

    #[test]
    fn echo_canceller_removes_the_reference() {
        // The mic hears exactly what the speakers play, 20 ms later and quieter.
        let len = RATE as usize * 6;
        let far = noise(len + 960, 0.3);
        let mic: Vec<f32> = far[..len].iter().map(|s| s * 0.5).collect();
        let fx = Effects { echo: true, ..Effects::default() };
        let mut p = Processor::new(&fx, 1);
        let mut out = vec![0.0; len];
        for (i, (m, o)) in mic.chunks(FRAME).zip(out.chunks_mut(FRAME)).enumerate() {
            p.push_reference(far[960 + i * FRAME..960 + (i + 1) * FRAME].iter().copied());
            p.push_input(m.iter().copied());
            p.pull(o);
        }
        let tail = &out[len - RATE as usize..];
        assert!(rms(tail) < rms(&mic) * 0.3, "echo left: {} of {}", rms(tail), rms(&mic));
    }

    #[test]
    fn stereo_keeps_channels_apart() {
        // Music in the left channel only, with every stereo-safe effect on.
        let left: Vec<f32> = (0..RATE as usize).map(|i| 0.4 * (TAU * 330.0 * i as f32 / RATE as f32).sin()).collect();
        let input: Vec<f32> = left.iter().flat_map(|&l| [l, 0.0]).collect();
        let fx = Effects {
            noise: NoiseMode::Standard,
            auto_gain: true,
            compressor: 0.5,
            gate: true,
            bass_db: 4.0,
            pitch: 3.0,
            voice: VoiceEffect::Hall,
            ..Effects::default()
        };
        let mut p = Processor::new(&fx, 2);
        let mut out = vec![0.0; input.len()];
        for (i, o) in input.chunks(FRAME * 2).zip(out.chunks_mut(FRAME * 2)) {
            p.push_input(i.iter().copied());
            p.pull(o);
        }
        let half = out.len() / 2;
        let l: Vec<f32> = out[half..].iter().step_by(2).copied().collect();
        let r: Vec<f32> = out[half + 1..].iter().step_by(2).copied().collect();
        assert!(rms(&l) > 0.05, "left too quiet: {}", rms(&l));
        assert!(rms(&r) < rms(&l) * 0.05, "left leaked into right: {} vs {}", rms(&r), rms(&l));
    }

    #[test]
    fn output_queue_does_not_grow_without_a_reader() {
        let mut p = Processor::new(&Effects::default(), 1);
        p.push_input(std::iter::repeat_n(0.1, RATE as usize * 5));
        assert!(p.output.len() <= FRAME * 6);
    }
}
