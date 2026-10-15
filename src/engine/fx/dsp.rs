//! Small, allocation-free signal processing blocks for the effects chain.
//! Everything works on mono `f32` samples at [`RATE`].

use crate::config::VoiceEffect;
use std::f32::consts::{PI, TAU};

pub const RATE: f32 = 48_000.0;

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.max(1e-9).log10()
}

/// One-pole smoothing coefficient for a time constant in seconds.
fn coef(seconds: f32) -> f32 {
    (-1.0 / (seconds * RATE)).exp()
}

/// RBJ cookbook biquad in transposed direct form II.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    fn from(b: [f32; 3], a: [f32; 3]) -> Self {
        Self { b0: b[0] / a[0], b1: b[1] / a[0], b2: b[2] / a[0], a1: a[1] / a[0], a2: a[2] / a[0], ..Self::default() }
    }

    fn w(freq: f32, q: f32) -> (f32, f32) {
        let w = TAU * freq / RATE;
        (w.cos(), w.sin() / (2.0 * q))
    }

    pub fn lowpass(freq: f32, q: f32) -> Self {
        let (cos, alpha) = Self::w(freq, q);
        Self::from([(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub fn highpass(freq: f32, q: f32) -> Self {
        let (cos, alpha) = Self::w(freq, q);
        Self::from([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub fn peaking(freq: f32, q: f32, db: f32) -> Self {
        let (cos, alpha) = Self::w(freq, q);
        let a = 10f32.powf(db / 40.0);
        Self::from(
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        )
    }

    fn shelf(freq: f32, db: f32, high: bool) -> Self {
        let a = 10f32.powf(db / 40.0);
        let (cos, alpha) = Self::w(freq, std::f32::consts::FRAC_1_SQRT_2);
        let s = 2.0 * a.sqrt() * alpha;
        let (p, m) = (a + 1.0, a - 1.0);
        if high {
            Self::from(
                [a * (p + m * cos + s), -2.0 * a * (m + p * cos), a * (p + m * cos - s)],
                [p - m * cos + s, 2.0 * (m - p * cos), p - m * cos - s],
            )
        } else {
            Self::from(
                [a * (p - m * cos + s), 2.0 * a * (m - p * cos), a * (p - m * cos - s)],
                [p + m * cos + s, -2.0 * (m + p * cos), p + m * cos - s],
            )
        }
    }

    pub fn low_shelf(freq: f32, db: f32) -> Self {
        Self::shelf(freq, db, false)
    }

    pub fn high_shelf(freq: f32, db: f32) -> Self {
        Self::shelf(freq, db, true)
    }

    /// Take new coefficients but keep the filter state, so changing a
    /// setting does not click.
    pub fn retune(&mut self, other: Self) {
        *self = Self { z1: self.z1, z2: self.z2, ..other };
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// A value that glides to its target over a few milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct Smoothed {
    current: f32,
    pub target: f32,
}

impl Smoothed {
    pub fn new(value: f32) -> Self {
        Self { current: value, target: value }
    }

    pub fn next(&mut self) -> f32 {
        self.current = self.target + (self.current - self.target) * 0.998;
        self.current
    }
}

/// Noise gate with hold and hysteresis: opens quickly when the level crosses
/// the threshold and fades out once it has stayed below it for a while.
#[derive(Debug)]
pub struct Gate {
    envelope: f32,
    gain: f32,
    hold: u32,
}

impl Default for Gate {
    fn default() -> Self {
        Self { envelope: 0.0, gain: 1.0, hold: 0 }
    }
}

impl Gate {
    const HOLD: f32 = 0.2;
    const HYSTERESIS_DB: f32 = 6.0;

    #[cfg(test)]
    pub fn process(&mut self, x: f32, threshold_db: f32) -> f32 {
        x * self.gain(x.abs(), threshold_db)
    }

    /// Gain to apply for a sample whose peak (over all channels) is `a`.
    pub fn gain(&mut self, a: f32, threshold_db: f32) -> f32 {
        self.envelope = if a > self.envelope { a } else { self.envelope * coef(0.05) };
        let level = gain_to_db(self.envelope);
        let open = if self.gain > 0.5 { threshold_db - Self::HYSTERESIS_DB } else { threshold_db };
        let target = if level > open {
            self.hold = (Self::HOLD * RATE) as u32;
            1.0
        } else if self.hold > 0 {
            self.hold -= 1;
            1.0
        } else {
            0.0
        };
        let c = if target > self.gain { coef(0.002) } else { coef(0.08) };
        self.gain = target + (self.gain - target) * c;
        self.gain
    }
}

/// Feed-forward compressor whose threshold, ratio and make-up gain all follow
/// one "amount" knob.
#[derive(Debug, Default)]
pub struct Compressor {
    reduction_db: f32,
}

impl Compressor {
    #[cfg(test)]
    pub fn process(&mut self, x: f32, amount: f32) -> f32 {
        x * self.gain(x.abs(), amount)
    }

    /// Gain to apply for a sample whose peak (over all channels) is `a`.
    pub fn gain(&mut self, a: f32, amount: f32) -> f32 {
        let threshold = -12.0 - 18.0 * amount;
        let slope = 1.0 - 1.0 / (1.0 + 5.0 * amount);
        let over = gain_to_db(a) - threshold;
        let target = if over > 0.0 { over * slope } else { 0.0 };
        let c = if target > self.reduction_db { coef(0.005) } else { coef(0.15) };
        self.reduction_db = target + (self.reduction_db - target) * c;
        let makeup = -threshold * slope * 0.5;
        db_to_gain(makeup - self.reduction_db)
    }
}

/// Peak limiter that keeps the output just below full scale.
#[derive(Debug)]
pub struct Limiter {
    gain: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl Limiter {
    const CEILING: f32 = 0.89; // -1 dBFS

    #[cfg(test)]
    pub fn process(&mut self, x: f32) -> f32 {
        (x * self.gain(x.abs())).clamp(-1.0, 1.0)
    }

    /// Gain to apply for a sample whose peak (over all channels) is `a`.
    pub fn gain(&mut self, a: f32) -> f32 {
        let needed = if a > Self::CEILING { Self::CEILING / a } else { 1.0 };
        self.gain = (1.0 + (self.gain - 1.0) * coef(0.05)).min(needed);
        self.gain
    }
}

/// A circular buffer read at fractional delays.
#[derive(Debug)]
struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    fn new(max_seconds: f32) -> Self {
        let len = ((max_seconds * RATE) as usize + 2).next_power_of_two();
        Self { buf: vec![0.0; len], pos: 0 }
    }

    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & (self.buf.len() - 1);
        self.buf[self.pos] = x;
    }

    /// The sample written `delay` samples ago (linear interpolation).
    fn read(&self, delay: f32) -> f32 {
        let mask = self.buf.len() - 1;
        let d = delay.clamp(0.0, (self.buf.len() - 2) as f32);
        let i = d as usize;
        let frac = d - i as f32;
        let a = self.buf[(self.pos + self.buf.len() - i) & mask];
        let b = self.buf[(self.pos + self.buf.len() - i - 1) & mask];
        a + (b - a) * frac
    }
}

/// Granular pitch shifter: two read heads sweep through a delay line at the
/// new speed and cross-fade so that each jump back is hidden.
#[derive(Debug)]
pub struct PitchShifter {
    line: DelayLine,
    phase: f32,
}

impl Default for PitchShifter {
    fn default() -> Self {
        Self { line: DelayLine::new(0.1), phase: 0.0 }
    }
}

impl PitchShifter {
    /// Grain length; longer is smoother for low voices but smears more.
    const WINDOW: f32 = 0.04 * RATE;

    pub fn process(&mut self, x: f32, semitones: f32) -> f32 {
        self.line.push(x);
        let ratio = 2f32.powf(semitones / 12.0);
        self.phase = (self.phase + (1.0 - ratio) / Self::WINDOW).rem_euclid(1.0);
        let other = (self.phase + 0.5) % 1.0;
        // sin² fades of two heads half a grain apart always sum to one.
        let fade = |p: f32| (PI * p).sin().powi(2);
        self.line.read(1.0 + self.phase * Self::WINDOW) * fade(self.phase)
            + self.line.read(1.0 + other * Self::WINDOW) * fade(other)
    }
}

/// Mono reverb after Freeverb: parallel damped combs into series all-passes.
#[derive(Debug)]
struct Reverb {
    combs: Vec<(DelayLine, usize, f32)>,
    allpasses: Vec<(DelayLine, usize)>,
}

impl Reverb {
    fn new() -> Self {
        let scale = RATE / 44_100.0;
        let len = |n: f32| (n * scale) as usize;
        Self {
            combs: [1557.0, 1617.0, 1491.0, 1422.0, 1277.0, 1356.0]
                .iter()
                .map(|&n| (DelayLine::new(0.05), len(n), 0.0))
                .collect(),
            allpasses: [556.0, 441.0].iter().map(|&n| (DelayLine::new(0.02), len(n))).collect(),
        }
    }

    fn process(&mut self, x: f32, feedback: f32, damping: f32) -> f32 {
        let mut out = 0.0;
        for (line, len, filtered) in &mut self.combs {
            let y = line.read(*len as f32 - 1.0);
            *filtered = y * (1.0 - damping) + *filtered * damping;
            line.push(x * 0.15 + *filtered * feedback);
            out += y;
        }
        for (line, len) in &mut self.allpasses {
            let delayed = line.read(*len as f32 - 1.0);
            line.push(out + delayed * 0.5);
            out = delayed - out * 0.5;
        }
        out
    }
}

/// Character effects ("voice changer"). Each returns the wet signal; the
/// caller mixes it with the dry one.
#[derive(Debug)]
pub struct VoiceFx {
    current: VoiceEffect,
    filters: Vec<Biquad>,
    phase: f32,
    lfo: f32,
    line: DelayLine,
    reverb: Reverb,
}

impl Default for VoiceFx {
    fn default() -> Self {
        Self {
            current: VoiceEffect::None,
            filters: vec![],
            phase: 0.0,
            lfo: 0.0,
            line: DelayLine::new(0.5),
            reverb: Reverb::new(),
        }
    }
}

impl VoiceFx {
    fn set(&mut self, effect: VoiceEffect) {
        if effect == self.current {
            return;
        }
        // Start from silence so the old effect's tail does not leak into the new one.
        *self = Self { current: effect, ..Self::default() };
        use Biquad as B;
        self.filters = match effect {
            VoiceEffect::Radio => vec![B::highpass(350.0, 0.7), B::lowpass(2800.0, 0.9), B::peaking(1500.0, 1.0, 4.0)],
            VoiceEffect::Telephone => vec![B::highpass(400.0, 0.9), B::highpass(400.0, 0.9), B::lowpass(3400.0, 0.9)],
            VoiceEffect::Megaphone => vec![B::highpass(600.0, 0.8), B::lowpass(4000.0, 0.8), B::peaking(1800.0, 1.2, 9.0)],
            VoiceEffect::Underwater => vec![B::lowpass(500.0, 1.2), B::lowpass(700.0, 0.7)],
            VoiceEffect::Robot => vec![B::highpass(150.0, 0.7)],
            _ => vec![],
        };
    }

    fn filter(&mut self, x: f32) -> f32 {
        self.filters.iter_mut().fold(x, |x, f| f.process(x))
    }

    /// Advance a sine oscillator kept in `phase` (0..1) and return its value.
    fn osc(phase: &mut f32, freq: f32) -> f32 {
        *phase = (*phase + freq / RATE) % 1.0;
        (TAU * *phase).sin()
    }

    pub fn process(&mut self, x: f32, effect: VoiceEffect) -> f32 {
        self.set(effect);
        match effect {
            VoiceEffect::None => x,
            VoiceEffect::Robot => {
                // Ring modulation plus a short resonant comb gives the metallic buzz.
                let ring = x * Self::osc(&mut self.phase, 50.0);
                let comb = self.line.read(0.006 * RATE);
                let y = self.filter(ring * 0.7 + x * 0.3) + comb * 0.6;
                self.line.push(y);
                y * 0.8
            }
            VoiceEffect::Alien => {
                let vibrato = Self::osc(&mut self.lfo, 6.0);
                self.line.push(x);
                let wobbly = self.line.read(0.004 * RATE + vibrato * 0.002 * RATE);
                wobbly * Self::osc(&mut self.phase, 330.0) * 1.2
            }
            VoiceEffect::Radio => (self.filter(x) * 3.0).tanh() * 0.6,
            VoiceEffect::Telephone => (self.filter(x) * 1.8).tanh() * 0.8,
            VoiceEffect::Megaphone => (self.filter(x) * 5.0).tanh() * 0.45,
            VoiceEffect::Underwater => {
                let wobble = Self::osc(&mut self.lfo, 0.8);
                let filtered = self.filter(x);
                self.line.push(filtered);
                self.line.read(0.006 * RATE + wobble * 0.004 * RATE) * 1.4
            }
            VoiceEffect::Echo => {
                let delayed = self.line.read(0.28 * RATE);
                self.line.push(x + delayed * 0.4);
                x + delayed * 0.5
            }
            VoiceEffect::Hall => x + self.reverb.process(x, 0.86, 0.25) * 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE) as usize).map(|i| amp * (TAU * freq * i as f32 / RATE).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Dominant frequency by counting upward zero crossings.
    fn frequency(x: &[f32]) -> f32 {
        let crossings = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f32 * RATE / x.len() as f32
    }

    #[test]
    fn shelves_and_peaks_have_the_requested_gain() {
        let gain_at = |mut f: Biquad, freq: f32| {
            let out: Vec<f32> = sine(freq, 0.5, 0.5).into_iter().map(|s| f.process(s)).collect();
            gain_to_db(rms(&out[12_000..]) / rms(&sine(freq, 0.5, 0.25)))
        };
        assert!((gain_at(Biquad::low_shelf(150.0, 6.0), 40.0) - 6.0).abs() < 0.5);
        assert!(gain_at(Biquad::low_shelf(150.0, 6.0), 5000.0).abs() < 0.5);
        assert!((gain_at(Biquad::high_shelf(6000.0, -6.0), 15_000.0) + 6.0).abs() < 0.7);
        assert!((gain_at(Biquad::peaking(1000.0, 1.0, 9.0), 1000.0) - 9.0).abs() < 0.3);
    }

    #[test]
    fn gate_closes_on_quiet_input_and_opens_on_loud() {
        let mut gate = Gate::default();
        let quiet: Vec<f32> = sine(300.0, 0.001, 1.0).into_iter().map(|s| gate.process(s, -40.0)).collect();
        assert!(rms(&quiet[24_000..]) < 1e-5);
        let loud: Vec<f32> = sine(300.0, 0.3, 0.5).into_iter().map(|s| gate.process(s, -40.0)).collect();
        assert!(rms(&loud[2_400..]) > 0.2);
    }

    #[test]
    fn limiter_keeps_peaks_below_full_scale() {
        let mut limiter = Limiter::default();
        let out: Vec<f32> = sine(200.0, 3.0, 0.3).into_iter().map(|s| limiter.process(s)).collect();
        assert!(out.iter().all(|s| s.abs() <= Limiter::CEILING + 1e-6));
    }

    #[test]
    fn compressor_reduces_dynamic_range() {
        let level = |amp: f32| {
            let mut c = Compressor::default();
            let out: Vec<f32> = sine(300.0, amp, 0.5).into_iter().map(|s| c.process(s, 1.0)).collect();
            gain_to_db(rms(&out[12_000..]))
        };
        let spread_in = gain_to_db(0.5) - gain_to_db(0.05);
        assert!(level(0.5) - level(0.05) < spread_in / 2.0);
    }

    #[test]
    fn pitch_shifter_moves_an_octave() {
        for (semitones, expected) in [(12.0, 400.0), (-12.0, 100.0), (0.0, 200.0)] {
            let mut p = PitchShifter::default();
            let out: Vec<f32> = sine(200.0, 0.5, 1.0).into_iter().map(|s| p.process(s, semitones)).collect();
            let f = frequency(&out[4_800..]);
            assert!((f - expected).abs() < expected * 0.08, "{semitones} st: got {f} Hz");
        }
    }

    #[test]
    fn every_voice_effect_stays_finite_and_audible() {
        use VoiceEffect::*;
        for effect in [None, Robot, Alien, Radio, Telephone, Megaphone, Underwater, Echo, Hall] {
            let mut fx = VoiceFx::default();
            let out: Vec<f32> = sine(440.0, 0.5, 0.5).into_iter().map(|s| fx.process(s, effect)).collect();
            assert!(out.iter().all(|s| s.is_finite()), "{effect:?}");
            assert!(rms(&out) > 0.01, "{effect:?} is silent");
            assert!(out.iter().all(|s| s.abs() < 4.0), "{effect:?} blows up");
        }
    }
}
