//! Audio output (L0): the single sound-card path the app uses (slideshow music, video playback),
//! and [`AudioClock`], the device's position, which playback can follow instead of the system
//! clock so pictures stay in step with the sound.
//!
//! Without the `audio-out` cargo feature (the default) nothing links to a sound library:
//! [`CAN_PLAY`] is false and [`Output::start`] returns an error, so callers play silently.
//! With it, output goes through cpal (on Linux that links the C library libasound).
//!
//! Sources: own design (a sample counter advanced by the device callback).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Whether this build can play sound.
pub const CAN_PLAY: bool = cfg!(feature = "audio-out");

/// Fills an interleaved output buffer; called on the audio thread.
pub type Fill = Box<dyn FnMut(&mut [f32]) + Send>;

/// Frames handed to the output device, as seconds. Clones share one counter; it only moves while
/// the device pulls samples (a stalled or stopped stream freezes it), so it is monotonic.
#[derive(Clone, Debug)]
pub struct AudioClock {
    frames: Arc<AtomicU64>,
    rate: u32,
}

impl AudioClock {
    /// A clock at `rate` frames per second, at zero.
    pub fn new(rate: u32) -> Self {
        Self { frames: Arc::new(AtomicU64::new(0)), rate: rate.max(1) }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Count `frames` more as played.
    pub fn advance(&self, frames: u64) {
        let _ = self.frames.try_update(Ordering::Relaxed, Ordering::Relaxed, |f| Some(f.saturating_add(frames)));
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// Seconds played since the clock started.
    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }
}

/// The default output device's preferred sample rate (None without a device or the feature).
pub fn default_rate() -> Option<u32> {
    #[cfg(feature = "audio-out")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let d = cpal::default_host().default_output_device()?;
        d.default_output_config().ok().map(|c| c.sample_rate().0)
    }
    #[cfg(not(feature = "audio-out"))]
    {
        None
    }
}

/// A stream playing on the default output device; dropping it stops the sound.
pub struct Output {
    #[cfg(feature = "audio-out")]
    _stream: cpal::Stream,
    clock: AudioClock,
}

impl Output {
    /// Start playing interleaved f32 at `rate` Hz with `channels` channels, pulling samples from
    /// `fill` (which should write silence when it has nothing). Errors without the feature or a device.
    pub fn start(rate: u32, channels: u16, fill: Fill) -> Result<Output, String> {
        if rate == 0 || channels == 0 {
            return Err(format!("audio output: invalid format ({rate} Hz, {channels} channels)"));
        }
        let clock = AudioClock::new(rate);
        #[cfg(feature = "audio-out")]
        {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let mut fill = fill;
            let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
            let config = cpal::StreamConfig { channels, sample_rate: cpal::SampleRate(rate), buffer_size: cpal::BufferSize::Default };
            let c = clock.clone();
            let n = u64::from(channels);
            let stream = device
                .build_output_stream(
                    &config,
                    move |out: &mut [f32], _| {
                        out.fill(0.0);
                        fill(out);
                        c.advance(out.len() as u64 / n);
                    },
                    |e| log::warn!("audio output: {e}"),
                    None,
                )
                .map_err(|e| format!("audio output: {e}"))?;
            stream.play().map_err(|e| format!("audio output: {e}"))?;
            Ok(Output { _stream: stream, clock })
        }
        #[cfg(not(feature = "audio-out"))]
        {
            let _ = (fill, clock);
            Err("this build plays without sound (built without the audio-out feature)".into())
        }
    }

    /// The device clock of this stream.
    pub fn clock(&self) -> AudioClock {
        self.clock.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_counts_frames_and_saturates() {
        let c = AudioClock::new(48_000);
        let d = c.clone();
        d.advance(24_000);
        assert!((c.seconds() - 0.5).abs() < 1e-12);
        c.advance(u64::MAX);
        assert_eq!(d.frames(), u64::MAX);
        assert_eq!(AudioClock::new(0).rate(), 1);
    }

    #[test]
    fn start_without_the_feature_or_with_a_bad_format_is_an_error() {
        assert!(Output::start(0, 2, Box::new(|_| {})).is_err());
        if !CAN_PLAY {
            assert!(Output::start(48_000, 2, Box::new(|_| {})).is_err());
            assert_eq!(default_rate(), None);
        }
    }
}
