//! Safe wrapper around WebRTC's AudioProcessing module (see `cpp/apm.cpp`).

use std::ffi::c_int;
use std::ptr::NonNull;

#[repr(C)]
struct RawApm {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn hepcat_apm_new(rate: c_int, channels: c_int) -> *mut RawApm;
    fn hepcat_apm_free(apm: *mut RawApm);
    fn hepcat_apm_configure(apm: *mut RawApm, echo: bool, noise_level: c_int, auto_gain: bool);
    fn hepcat_apm_capture(apm: *mut RawApm, planes: *const *mut f32) -> c_int;
    fn hepcat_apm_render(apm: *mut RawApm, frame: *mut f32) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApmConfig {
    pub echo: bool,
    /// WebRTC suppression level 0 (low) ..= 3 (very high).
    pub noise: Option<u8>,
    pub auto_gain: bool,
}

/// Works on 10 ms frames at the rate it was created with: `channels` planes
/// of microphone (or app) audio, and a mono echo reference.
pub struct Apm {
    raw: NonNull<RawApm>,
    frame: usize,
    channels: usize,
}

impl Apm {
    pub fn new(rate: u32, channels: usize) -> Option<Self> {
        // SAFETY: plain constructor; null on failure.
        let raw = NonNull::new(unsafe { hepcat_apm_new(rate as c_int, channels as c_int) })?;
        Some(Self { raw, frame: rate as usize / 100, channels })
    }

    pub fn configure(&mut self, config: ApmConfig) {
        let noise = config.noise.map_or(-1, |n| c_int::from(n.min(3)));
        // SAFETY: `raw` is valid until drop.
        unsafe { hepcat_apm_configure(self.raw.as_ptr(), config.echo, noise, config.auto_gain) }
    }

    /// Process one frame in place, one plane per channel.
    pub fn capture<const N: usize>(&mut self, planes: &mut [[f32; N]]) {
        assert_eq!(planes.len(), self.channels);
        assert_eq!(N, self.frame);
        let ptrs: Vec<*mut f32> = planes.iter_mut().map(|p| p.as_mut_ptr()).collect();
        // SAFETY: one pointer per configured channel, each to exactly 10 ms of samples.
        unsafe { hepcat_apm_capture(self.raw.as_ptr(), ptrs.as_ptr()) };
    }

    /// Feed one frame of what is being played, as the echo reference.
    pub fn render(&mut self, frame: &mut [f32]) {
        assert_eq!(frame.len(), self.frame);
        // SAFETY: as above.
        unsafe { hepcat_apm_render(self.raw.as_ptr(), frame.as_mut_ptr()) };
    }
}

// SAFETY: the processor is only ever used by one thread at a time (it lives
// behind the Processor's mutex); WebRTC does not tie it to a thread.
unsafe impl Send for Apm {}

impl Drop for Apm {
    fn drop(&mut self) {
        // SAFETY: created by hepcat_apm_new and freed only here.
        unsafe { hepcat_apm_free(self.raw.as_ptr()) }
    }
}
