//! The PipeWire side of a strip's effects: a capture stream for the audio to
//! process, an output stream for the result, and depending on the strip:
//!
//! - a microphone gets a virtual microphone, "<mic> (Hepcat FX)", that the
//!   output stream plays into and apps record from, plus (for echo
//!   cancellation) a capture stream of what the speakers play;
//! - an app gets a virtual output, "<app> (Hepcat FX)", that WirePlumber
//!   sends it to instead of its first bus; the output stream then plays into
//!   the app's buses;
//! - a bus needs nothing extra: its monitor is captured and the output stream
//!   plays to the bus's device.
//!
//! The virtual devices are null nodes like the buses and bus microphones. (An
//! output stream that calls itself a source gets its ports reconfigured by
//! WirePlumber, which crashes PipeWire 1.6 while a link is being made.) They
//! do not linger: when Hepcat quits, apps fall back to working devices
//! instead of recording or playing into nothing.
//!
//! The streams process audio on PipeWire's realtime thread (a busy main
//! thread would otherwise make the output miss cycles and drop out) and share
//! one [`Processor`] behind a mutex, which the main thread only holds briefly
//! to change settings or read the level.

use super::{Processor, RATE};
use crate::config::{Effects, FxTarget};
use crate::engine::format::{MONO, STEREO, f32_audio};
use crate::engine::meter::display_level;
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::spa::pod::Pod;
use std::sync::{Arc, Mutex};

/// node.name prefix of the virtual device of a microphone or app with effects.
pub const FX_PREFIX: &str = "hepcat.fx.";
/// node.name prefix of the stream that captures the audio to process.
pub const FX_IN_PREFIX: &str = "hepcat.fxin.";
/// node.name prefix of the stream that captures the echo reference.
pub const FX_REF_PREFIX: &str = "hepcat.fxref.";
/// node.name prefix of the stream that plays the processed audio.
pub const FX_OUT_PREFIX: &str = "hepcat.fxout.";

/// node.names of everything that makes up the effects of one strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxNames {
    /// The virtual microphone or output (unused for buses).
    pub device: String,
    pub input: String,
    pub output: String,
    pub reference: String,
}

impl FxNames {
    /// For a key of [`crate::config::Config::fx`], e.g. `app:Firefox` gives
    /// `hepcat.fx.app.Firefox` and so on.
    pub fn new(key: &str) -> Self {
        let slug: String = key.chars().map(|c| if c == ':' { '.' } else if c.is_whitespace() { '_' } else { c }).collect();
        Self {
            device: format!("{FX_PREFIX}{slug}"),
            input: format!("{FX_IN_PREFIX}{slug}"),
            output: format!("{FX_OUT_PREFIX}{slug}"),
            reference: format!("{FX_REF_PREFIX}{slug}"),
        }
    }
}

type Shared = Arc<Mutex<Processor>>;

/// Kept only to keep the stream alive.
struct Stream {
    _stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<Shared>,
}

pub struct FxNode {
    processor: Shared,
    _input: Stream,
    _output: Stream,
    _device: Option<pw::node::Node>,
    reference: Option<Stream>,
    names: FxNames,
    level: f32,
}

impl FxNode {
    /// `key` says what to process (see [`crate::config::Config::fx`]),
    /// `label` is what the user calls it.
    pub fn new(core: &pw::core::CoreRc, key: &str, label: &str, fx: &Effects) -> Result<Self, pw::Error> {
        let target = FxTarget::parse(key).ok_or(pw::Error::CreationFailed)?;
        let names = FxNames::new(key);
        let processor = Arc::new(Mutex::new(Processor::new(fx, target.channels())));
        let input = capture(
            core,
            &processor,
            properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                // Not "Stream/Input/Audio", so it is not listed as a recording app.
                *pw::keys::MEDIA_CLASS => "Stream/Input/Audio/Effects",
                *pw::keys::NODE_NAME => names.input.clone(),
                "node.description" => format!("Hepcat effects for {label}"),
                "node.autoconnect" => "false",
                "node.dont-reconnect" => "true",
                // Input and output must run on the same clock; with two
                // drivers the queue between them drifts and the sound drops out.
                "node.group" => names.output.clone(),
            },
            false,
        )?;
        let output = playback(core, &processor, &names.output)?;
        let device = match target {
            FxTarget::Mic(_) => Some(core.create_object::<pw::node::Node>(
                "adapter",
                &properties! {
                    "factory.name" => "support.null-audio-sink",
                    *pw::keys::NODE_NAME => names.device.clone(),
                    "node.description" => format!("{label} (Hepcat FX)"),
                    *pw::keys::MEDIA_CLASS => "Audio/Source/Virtual",
                    "audio.position" => "FL,FR",
                    "node.virtual" => "true",
                    "device.icon-name" => "audio-input-microphone",
                },
            )?),
            FxTarget::App(_) => Some(core.create_object::<pw::node::Node>(
                "adapter",
                &properties! {
                    "factory.name" => "support.null-audio-sink",
                    *pw::keys::NODE_NAME => names.device.clone(),
                    "node.description" => format!("{label} (Hepcat FX)"),
                    *pw::keys::MEDIA_CLASS => "Audio/Sink",
                    "audio.position" => "FL,FR",
                    "node.virtual" => "true",
                    // Never picked as the default output.
                    "priority.session" => "1",
                },
            )?),
            FxTarget::Bus(_) => None,
        };
        let mut node =
            Self { processor, _input: input, _output: output, _device: device, reference: None, names, level: 0.0 };
        node.update(core, fx);
        Ok(node)
    }

    pub fn update(&mut self, core: &pw::core::CoreRc, fx: &Effects) {
        let Ok(mut processor) = self.processor.lock() else { return };
        processor.set_effects(fx);
        let wants_reference = processor.wants_reference();
        drop(processor);
        if wants_reference && self.reference.is_none() {
            let props = properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Monitor",
                *pw::keys::MEDIA_CLASS => "Stream/Input/Audio/Effects",
                *pw::keys::NODE_NAME => self.names.reference.clone(),
                "node.description" => "Hepcat echo reference",
                "node.autoconnect" => "false",
                "node.dont-reconnect" => "true",
                // Like meters: listening to the speakers must not keep them awake.
                "node.passive" => "true",
            };
            match capture(core, &self.processor, props, true) {
                Ok(stream) => self.reference = Some(stream),
                Err(e) => eprintln!("hepcat: failed to create echo reference {}: {e}", self.names.reference),
            }
        } else if !wants_reference {
            self.reference = None;
        }
    }

    /// Output level for the meter, like [`crate::engine::meter::Meter::take_level`].
    pub fn take_level(&mut self) -> f32 {
        let peak = self.processor.lock().map_or(0.0, |mut p| p.take_peak());
        display_level(peak, &mut self.level)
    }
}

fn connect(stream: &pw::stream::StreamRc, direction: spa::utils::Direction, positions: &[u32]) -> Result<(), pw::Error> {
    let bytes = f32_audio(positions, Some(RATE))?;
    let mut params = [Pod::from_bytes(&bytes).ok_or(pw::Error::CreationFailed)?];
    let flags = pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS;
    stream.connect(direction, None, flags, &mut params)
}

/// A stereo capture stream that feeds the processor, as input (downmixed if
/// the processor is mono) or as echo reference (always mono).
fn capture(
    core: &pw::core::CoreRc,
    processor: &Shared,
    props: pw::properties::PropertiesBox,
    reference: bool,
) -> Result<Stream, pw::Error> {
    let stream = pw::stream::StreamRc::new(core.clone(), "hepcat-fx", props)?;
    let listener = stream
        .add_local_listener_with_user_data(processor.clone())
        .process(move |stream, processor| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let Some(data) = buffer.datas_mut().first_mut() else { return };
            let (offset, size) = (data.chunk().offset() as usize, data.chunk().size() as usize);
            let Some(bytes) = data.data() else { return };
            let end = (offset + size).min(bytes.len());
            let bytes = &bytes[offset.min(end)..end];
            let sample = |b: &[u8]| f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let Ok(mut processor) = processor.lock() else { return };
            if reference || processor.channels() == 1 {
                let mono = bytes.chunks_exact(8).map(|b| (sample(&b[..4]) + sample(&b[4..])) * 0.5);
                if reference {
                    processor.push_reference(mono);
                } else {
                    processor.push_input(mono);
                }
            } else {
                processor.push_input(bytes.chunks_exact(4).map(sample));
            }
        })
        .register()?;
    connect(&stream, spa::utils::Direction::Input, &STEREO)?;
    Ok(Stream { _stream: stream, _listener: listener })
}

/// The processed audio; Hepcat links it to wherever it should go.
fn playback(core: &pw::core::CoreRc, processor: &Shared, name: &str) -> Result<Stream, pw::Error> {
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        // Not "Stream/Output/Audio", so it is not listed as a playing app.
        *pw::keys::MEDIA_CLASS => "Stream/Output/Audio/Effects",
        *pw::keys::NODE_NAME => name.to_owned(),
        "node.description" => "Hepcat effects output",
        "node.autoconnect" => "false",
        "node.dont-reconnect" => "true",
        "node.group" => name.to_owned(),
    };
    let stream = pw::stream::StreamRc::new(core.clone(), "hepcat-fx", props)?;
    let listener = stream
        .add_local_listener_with_user_data(processor.clone())
        .process(|stream, processor| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let requested = buffer.requested() as usize;
            let Some(data) = buffer.datas_mut().first_mut() else { return };
            let Some(bytes) = data.data() else { return };
            let Ok(mut processor) = processor.lock() else { return };
            let stride = 4 * processor.channels();
            let capacity = bytes.len() / stride;
            // `requested` counts frames (one sample per channel).
            let frames = if requested > 0 { requested.min(capacity) } else { capacity.min(1024) };
            processor.pull_into(bytes, frames);
            drop(processor);
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as i32;
            *chunk.size_mut() = (frames * stride) as u32;
        })
        .register()?;
    let mono = processor.lock().map_or(true, |p| p.channels() == 1);
    let positions: &[u32] = if mono { &MONO } else { &STEREO };
    connect(&stream, spa::utils::Direction::Output, positions)?;
    Ok(Stream { _stream: stream, _listener: listener })
}
