//! EnumFormat params for our own streams.

use pipewire as pw;
use pw::spa;

/// 32-bit float interleaved audio with the given channel positions, at
/// `rate` if one is given (PipeWire resamples) or the graph rate otherwise.
pub fn f32_audio(positions: &[u32], rate: Option<u32>) -> Result<Vec<u8>, pw::Error> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_channels(positions.len() as u32);
    if let Some(rate) = rate {
        info.set_rate(rate);
    }
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[..positions.len()].copy_from_slice(positions);
    info.set_position(position);
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    Ok(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj))
        .map_err(|_| pw::Error::CreationFailed)?
        .0
        .into_inner())
}

pub const STEREO: [u32; 2] = [spa::sys::SPA_AUDIO_CHANNEL_FL, spa::sys::SPA_AUDIO_CHANNEL_FR];
pub const MONO: [u32; 1] = [spa::sys::SPA_AUDIO_CHANNEL_MONO];
