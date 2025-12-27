//! Audio Device Trait
//!
//! Implemented by audio drivers (AC'97, HDA, etc.)
//! Used by the audio subsystem

use crate::DriverResult;

/// Audio sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 8-bit unsigned
    U8,
    /// 16-bit signed little-endian
    S16Le,
    /// 16-bit signed big-endian
    S16Be,
    /// 32-bit float
    F32,
}

/// Audio configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    /// Sample rate in Hz (e.g., 44100, 48000)
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u8,
    /// Sample format
    pub format: SampleFormat,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            channels: 2,
            format: SampleFormat::S16Le,
        }
    }
}

/// Audio device trait
pub trait AudioDevice: Send + Sync {
    /// Get current audio configuration
    fn config(&self) -> AudioConfig;

    /// Set audio configuration
    fn set_config(&mut self, config: AudioConfig) -> DriverResult<()>;

    /// Start audio playback
    fn start(&mut self) -> DriverResult<()>;

    /// Stop audio playback
    fn stop(&mut self) -> DriverResult<()>;

    /// Write audio samples to the device
    ///
    /// # Arguments
    /// * `samples` - Audio sample data
    ///
    /// # Returns
    /// Number of bytes written (may be less than samples.len() if buffer full)
    fn write(&mut self, samples: &[u8]) -> DriverResult<usize>;

    /// Get the number of bytes that can be written without blocking
    fn available(&self) -> usize;

    /// Set master volume (0-100)
    fn set_volume(&mut self, volume: u8) -> DriverResult<()>;

    /// Get master volume (0-100)
    fn volume(&self) -> u8;

    /// Mute/unmute
    fn set_mute(&mut self, muted: bool) -> DriverResult<()>;

    /// Check if muted
    fn is_muted(&self) -> bool;

    /// Get device information
    fn info(&self) -> AudioDeviceInfo;
}

/// Information about an audio device
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Device name
    pub name: &'static str,
    /// Current configuration
    pub config: AudioConfig,
    /// Buffer size in bytes
    pub buffer_size: usize,
    /// Is currently playing
    pub playing: bool,
}

/// Calculate bytes per sample for a format
pub fn bytes_per_sample(format: SampleFormat) -> usize {
    match format {
        SampleFormat::U8 => 1,
        SampleFormat::S16Le | SampleFormat::S16Be => 2,
        SampleFormat::F32 => 4,
    }
}

/// Calculate bytes per frame (sample * channels)
pub fn bytes_per_frame(config: &AudioConfig) -> usize {
    bytes_per_sample(config.format) * config.channels as usize
}
