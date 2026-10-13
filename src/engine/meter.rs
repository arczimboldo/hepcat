//! Level meters. Each metered node gets a tiny stereo capture stream that we
//! link to it ourselves (passively, so a meter never wakes up a device or a
//! microphone). The stream's process callback records the peak sample.

use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::spa::pod::Pod;
use std::cell::Cell;
use std::rc::Rc;

/// node.name prefix of meter streams; the metered node id follows it.
pub const METER_PREFIX: &str = "hepcat.meter.";

pub struct Meter {
    stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<Rc<Cell<f32>>>,
    peak: Rc<Cell<f32>>,
    /// Displayed level, 0..1 on a -60..0 dB scale, with falloff.
    level: f32,
}

impl Meter {
    pub fn new(core: &pw::core::CoreRc, target: u32) -> Result<Self, pw::Error> {
        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Monitor",
            // Not "Stream/Input/Audio": that would list every meter as a
            // recording app in volume applets and light up the mic indicator.
            *pw::keys::MEDIA_CLASS => "Stream/Input/Audio/Meter",
            *pw::keys::NODE_NAME => format!("{METER_PREFIX}{target}"),
            "node.description" => "Hepcat level meter",
            "node.autoconnect" => "false",
            "node.passive" => "true",
            "node.dont-reconnect" => "true",
            // ~20 ms chunks are smooth enough for a meter and keep wakeups cheap.
            "node.latency" => "1024/48000",
        };
        let stream = pw::stream::StreamRc::new(core.clone(), "hepcat-meter", props)?;
        let peak = Rc::new(Cell::new(0.0f32));
        let listener = stream
            .add_local_listener_with_user_data(peak.clone())
            .process(|stream, peak| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let Some(data) = buffer.datas_mut().first_mut() else { return };
                let size = data.chunk().size() as usize;
                let Some(bytes) = data.data() else { return };
                let max = bytes[..size.min(bytes.len())]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
                    .fold(0.0f32, f32::max);
                if max > peak.get() {
                    peak.set(max);
                }
            })
            .register()?;

        let mut info = spa::param::audio::AudioInfoRaw::new();
        info.set_format(spa::param::audio::AudioFormat::F32LE);
        // Stereo rather than mono: mixing L+R into one port would read 6 dB hot.
        info.set_channels(2);
        let mut position = [0; spa::param::audio::MAX_CHANNELS];
        position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
        position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
        info.set_position(position);
        let obj = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: info.into(),
        };
        let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(obj),
        )
        .map_err(|_| pw::Error::CreationFailed)?
        .0
        .into_inner();
        let mut params = [Pod::from_bytes(&bytes).ok_or(pw::Error::CreationFailed)?];
        stream.connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )?;

        Ok(Self { stream, _listener: listener, peak, level: 0.0 })
    }

    /// Our stream's node id, once PipeWire has assigned one.
    pub fn node_id(&self) -> u32 {
        self.stream.node_id()
    }

    /// Take the peak seen since the last call and return the level to show.
    pub fn take_level(&mut self) -> f32 {
        let peak = self.peak.replace(0.0);
        let instant = if peak > 0.0 { ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0) } else { 0.0 };
        // Rise instantly, fall smoothly.
        self.level = instant.max(self.level - 0.06).max(0.0);
        self.level
    }
}
