//! WATOS Generic Sound Card Driver
//!
//! Implements the AudioDevice trait for generic sound cards.
//! Provides basic PCM audio playback support with software mixing.
//!
//! # Usage
//!
//! ```rust,ignore
//! use watos_driver_audio_generic::GenericSoundDriver;
//! use watos_driver_traits::audio::{AudioDevice, AudioConfig};
//!
//! let mut driver = GenericSoundDriver::probe().expect("No sound card found");
//! driver.init().expect("Failed to initialize");
//! driver.start().expect("Failed to start");
//!
//! let config = AudioConfig::default(); // 44.1kHz, stereo, 16-bit
//! driver.set_config(config).expect("Failed to set config");
//!
//! let samples = vec![0u8; 4096];
//! driver.write(&samples).expect("Write failed");
//! ```

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use watos_driver_traits::{Driver, DriverInfo, DriverState, DriverError, DriverResult};
use watos_driver_traits::audio::{AudioDevice, AudioConfig, AudioDeviceInfo, SampleFormat};
use watos_driver_traits::bus::{PciAddress, PciBus, pci_class};
use watos_driver_pci::PciDriver;

/// Buffer size for audio samples (16KB)
const BUFFER_SIZE: usize = 16384;

/// Generic Sound Card Driver
pub struct GenericSoundDriver {
    state: DriverState,
    pci_addr: Option<PciAddress>,
    config: AudioConfig,
    buffer: [u8; BUFFER_SIZE],
    buffer_read_pos: usize,
    buffer_write_pos: usize,
    volume: u8,
    muted: bool,
    playing: bool,
}

impl GenericSoundDriver {
    /// Create a new uninitialized driver
    pub const fn new() -> Self {
        GenericSoundDriver {
            state: DriverState::Loaded,
            pci_addr: None,
            config: AudioConfig {
                sample_rate: 44100,
                channels: 2,
                format: SampleFormat::S16Le,
            },
            buffer: [0u8; BUFFER_SIZE],
            buffer_read_pos: 0,
            buffer_write_pos: 0,
            volume: 75,
            muted: false,
            playing: false,
        }
    }

    /// Probe for a compatible sound card on the PCI bus
    ///
    /// This looks for multimedia audio devices (class 0x04, subclass 0x01)
    pub fn probe() -> Option<Self> {
        let mut pci = PciDriver::new();
        
        // Initialize PCI driver
        if pci.init().is_err() {
            return None;
        }

        // Enumerate all PCI devices
        let devices = pci.enumerate();

        // Look for multimedia audio devices
        for device in devices {
            if device.id.class == pci_class::MULTIMEDIA && 
               device.id.subclass == pci_class::AUDIO {
                let mut driver = Self::new();
                driver.pci_addr = Some(device.address);
                driver.state = DriverState::Ready;
                return Some(driver);
            }
        }

        None
    }

    /// Probe for a sound card at a specific PCI address
    pub fn probe_at(addr: PciAddress) -> Option<Self> {
        let mut pci = PciDriver::new();
        
        if pci.init().is_err() {
            return None;
        }

        let devices = pci.enumerate();
        
        for device in devices {
            if device.address == addr && 
               device.id.class == pci_class::MULTIMEDIA && 
               device.id.subclass == pci_class::AUDIO {
                let mut driver = Self::new();
                driver.pci_addr = Some(addr);
                driver.state = DriverState::Ready;
                return Some(driver);
            }
        }

        None
    }

    /// Get available buffer space
    fn buffer_available(&self) -> usize {
        if self.buffer_write_pos >= self.buffer_read_pos {
            BUFFER_SIZE - (self.buffer_write_pos - self.buffer_read_pos) - 1
        } else {
            self.buffer_read_pos - self.buffer_write_pos - 1
        }
    }

    /// Write data to the ring buffer
    fn buffer_write(&mut self, data: &[u8]) -> usize {
        let available = self.buffer_available();
        let to_write = data.len().min(available);
        
        let mut written = 0;
        while written < to_write {
            self.buffer[self.buffer_write_pos] = data[written];
            self.buffer_write_pos = (self.buffer_write_pos + 1) % BUFFER_SIZE;
            written += 1;
        }
        
        written
    }

    /// Simulate draining the buffer (as if hardware consumed samples)
    /// In a real driver, this would be called by an interrupt handler
    fn buffer_drain(&mut self, count: usize) {
        let used = if self.buffer_write_pos >= self.buffer_read_pos {
            self.buffer_write_pos - self.buffer_read_pos
        } else {
            BUFFER_SIZE - self.buffer_read_pos + self.buffer_write_pos
        };
        
        let to_drain = count.min(used);
        self.buffer_read_pos = (self.buffer_read_pos + to_drain) % BUFFER_SIZE;
    }
}

impl Driver for GenericSoundDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            name: "Generic Sound Card",
            version: "0.1.0",
            author: "WATOS Project",
            description: "Generic software-based sound card driver with PCM playback support",
        }
    }

    fn state(&self) -> DriverState {
        self.state
    }

    fn init(&mut self) -> DriverResult<()> {
        if self.state != DriverState::Ready && self.state != DriverState::Loaded {
            return Err(DriverError::InvalidState);
        }

        // In a real driver, we would:
        // 1. Map PCI BARs to memory
        // 2. Allocate DMA buffers
        // 3. Initialize hardware registers
        // 4. Set up interrupts
        
        // For this generic driver, we just validate the PCI address
        if self.pci_addr.is_none() {
            return Err(DriverError::DeviceNotFound);
        }

        self.state = DriverState::Ready;
        Ok(())
    }

    fn start(&mut self) -> DriverResult<()> {
        if self.state != DriverState::Ready && self.state != DriverState::Stopped {
            return Err(DriverError::InvalidState);
        }

        // In a real driver, we would enable hardware DMA and interrupts
        self.state = DriverState::Active;
        Ok(())
    }

    fn stop(&mut self) -> DriverResult<()> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        // Stop playback and disable hardware
        self.playing = false;
        self.state = DriverState::Stopped;
        Ok(())
    }
}

impl AudioDevice for GenericSoundDriver {
    fn config(&self) -> AudioConfig {
        self.config
    }

    fn set_config(&mut self, config: AudioConfig) -> DriverResult<()> {
        // Validate configuration
        match config.format {
            SampleFormat::U8 | SampleFormat::S16Le | SampleFormat::S16Be => {},
            SampleFormat::F32 => return Err(DriverError::NotSupported),
        }

        if config.channels == 0 || config.channels > 2 {
            return Err(DriverError::InvalidParameter);
        }

        if config.sample_rate == 0 || config.sample_rate > 96000 {
            return Err(DriverError::InvalidParameter);
        }

        self.config = config;
        Ok(())
    }

    fn start(&mut self) -> DriverResult<()> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        if self.playing {
            // Already playing
            return Ok(());
        }

        self.playing = true;
        Ok(())
    }

    fn stop(&mut self) -> DriverResult<()> {
        if !self.playing {
            // Already stopped
            return Ok(());
        }

        self.playing = false;
        Ok(())
    }

    fn write(&mut self, samples: &[u8]) -> DriverResult<usize> {
        if !self.playing {
            return Err(DriverError::InvalidState);
        }

        if self.muted {
            // Still consume the samples but don't write them
            return Ok(samples.len());
        }

        // Apply volume scaling
        // For efficiency, we apply volume to the buffer as we write
        // Volume scaling: new_sample = (sample * volume) / 100
        let volume_scale = self.volume as u32;
        let mut scaled_samples = alloc::vec::Vec::with_capacity(samples.len());
        
        // Scale samples based on format
        match self.config.format {
            SampleFormat::S16Le | SampleFormat::S16Be => {
                // 16-bit samples: process in pairs
                for chunk in samples.chunks_exact(2) {
                    if chunk.len() == 2 {
                        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                        let scaled = ((sample as i32 * volume_scale as i32) / 100) as i16;
                        scaled_samples.push((scaled & 0xFF) as u8);
                        scaled_samples.push(((scaled >> 8) & 0xFF) as u8);
                    }
                }
            }
            SampleFormat::U8 => {
                // 8-bit samples: scale directly
                for &sample in samples {
                    let centered = sample as i32 - 128;
                    let scaled = (centered * volume_scale as i32) / 100;
                    scaled_samples.push((scaled + 128).clamp(0, 255) as u8);
                }
            }
            _ => {
                // For unsupported formats, just copy without scaling
                scaled_samples.extend_from_slice(samples);
            }
        }

        let written = self.buffer_write(&scaled_samples);

        // Simulate hardware consumption for testing
        // In a real driver, this would happen via interrupt
        self.buffer_drain(written / 4);

        Ok(written)
    }

    fn available(&self) -> usize {
        self.buffer_available()
    }

    fn set_volume(&mut self, volume: u8) -> DriverResult<()> {
        self.volume = volume.min(100);
        Ok(())
    }

    fn volume(&self) -> u8 {
        self.volume
    }

    fn set_mute(&mut self, muted: bool) -> DriverResult<()> {
        self.muted = muted;
        Ok(())
    }

    fn is_muted(&self) -> bool {
        self.muted
    }

    fn info(&self) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: "Generic Sound Card",
            config: self.config,
            buffer_size: BUFFER_SIZE,
            playing: self.playing,
        }
    }
}

/// Probe for all compatible sound cards on the system
pub fn probe_all() -> Vec<GenericSoundDriver> {
    let mut drivers = Vec::new();
    let mut pci = PciDriver::new();
    
    if pci.init().is_err() {
        return drivers;
    }

    let devices = pci.enumerate();

    for device in devices {
        if device.id.class == pci_class::MULTIMEDIA && 
           device.id.subclass == pci_class::AUDIO {
            let mut driver = GenericSoundDriver::new();
            driver.pci_addr = Some(device.address);
            driver.state = DriverState::Ready;
            drivers.push(driver);
        }
    }

    drivers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_operations() {
        let mut driver = GenericSoundDriver::new();
        
        // Initial state
        assert_eq!(driver.buffer_available(), BUFFER_SIZE - 1);
        
        // Write some data
        let data = [1u8, 2, 3, 4, 5];
        let written = driver.buffer_write(&data);
        assert_eq!(written, 5);
        assert_eq!(driver.buffer_available(), BUFFER_SIZE - 1 - 5);
        
        // Drain some data
        driver.buffer_drain(2);
        assert_eq!(driver.buffer_available(), BUFFER_SIZE - 1 - 3);
    }

    #[test]
    fn test_config_validation() {
        let mut driver = GenericSoundDriver::new();
        driver.state = DriverState::Active;
        
        // Valid config
        let config = AudioConfig {
            sample_rate: 48000,
            channels: 2,
            format: SampleFormat::S16Le,
        };
        assert!(driver.set_config(config).is_ok());
        
        // Invalid channels
        let bad_config = AudioConfig {
            sample_rate: 48000,
            channels: 0,
            format: SampleFormat::S16Le,
        };
        assert!(driver.set_config(bad_config).is_err());
        
        // Unsupported format
        let bad_config = AudioConfig {
            sample_rate: 48000,
            channels: 2,
            format: SampleFormat::F32,
        };
        assert!(driver.set_config(bad_config).is_err());
    }

    #[test]
    fn test_volume_control() {
        let mut driver = GenericSoundDriver::new();
        
        // Default volume
        assert_eq!(driver.volume(), 75);
        
        // Set volume
        assert!(driver.set_volume(50).is_ok());
        assert_eq!(driver.volume(), 50);
        
        // Clamp to max
        assert!(driver.set_volume(150).is_ok());
        assert_eq!(driver.volume(), 100);
        
        // Mute
        assert!(!driver.is_muted());
        assert!(driver.set_mute(true).is_ok());
        assert!(driver.is_muted());
    }
}
