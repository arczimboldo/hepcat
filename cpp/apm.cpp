// A tiny C interface to WebRTC's AudioProcessing module (the echo canceller,
// noise suppressor and gain control used by browsers and Discord). The
// library ships with PipeWire, which uses it for its own echo-cancel module.
// Builds against webrtc-audio-processing 1.x and 2.x.

#include <modules/audio_processing/include/audio_processing.h>

using webrtc::AudioProcessing;

struct HepcatApm {
    rtc::scoped_refptr<AudioProcessing> apm;
    webrtc::StreamConfig capture;
    webrtc::StreamConfig render;
};

extern "C" {

// `channels` is the capture channel count; the echo reference is always mono.
HepcatApm *hepcat_apm_new(int rate, int channels)
{
    // 1.x returns a raw pointer, 2.x a scoped_refptr; this takes either.
    rtc::scoped_refptr<AudioProcessing> apm(webrtc::AudioProcessingBuilder().Create());
    if (!apm.get())
        return nullptr;
    return new HepcatApm{apm, webrtc::StreamConfig(rate, channels), webrtc::StreamConfig(rate, 1)};
}

void hepcat_apm_free(HepcatApm *h)
{
    delete h;
}

// noise_level: -1 for off, otherwise 0 (low) .. 3 (very high).
void hepcat_apm_configure(HepcatApm *h, bool echo, int noise_level, bool auto_gain)
{
    AudioProcessing::Config c;
    // Without this, stereo is mixed down and comes out as mono.
    c.pipeline.multi_channel_capture = h->capture.num_channels() > 1;
    c.high_pass_filter.enabled = true;
    c.echo_canceller.enabled = echo;
    c.noise_suppression.enabled = noise_level >= 0;
    if (noise_level >= 0)
        c.noise_suppression.level = static_cast<AudioProcessing::Config::NoiseSuppression::Level>(noise_level);
    c.gain_controller2.enabled = auto_gain;
    c.gain_controller2.adaptive_digital.enabled = auto_gain;
    h->apm->ApplyConfig(c);
}

// One 10 ms frame, one plane per channel, processed in place.
int hepcat_apm_capture(HepcatApm *h, float *const *planes)
{
    return h->apm->ProcessStream(planes, h->capture, h->capture, planes);
}

// One 10 ms mono frame of what the speakers are playing, for echo cancellation.
int hepcat_apm_render(HepcatApm *h, float *frame)
{
    float *channels[1] = {frame};
    return h->apm->ProcessReverseStream(channels, h->render, h->render, channels);
}

}
