//! Example of using the generic sound driver
//!
//! This example demonstrates how to:
//! 1. Probe for sound cards
//! 2. Initialize the driver
//! 3. Configure audio settings
//! 4. Play audio samples

#![no_std]
#![no_main]

extern crate alloc;

use watos_driver_audio_generic::GenericSoundDriver;
use watos_driver_traits::{Driver, DriverState};
use watos_driver_traits::audio::{AudioDevice, AudioConfig, SampleFormat};

/// Example: Playing a simple tone
///
/// This function demonstrates basic usage of the sound driver:
/// 1. Probe for available sound cards
/// 2. Initialize and start the driver
/// 3. Configure audio format
/// 4. Write audio samples
pub fn play_tone_example() -> Result<(), &'static str> {
    // Probe for sound cards
    let mut driver = GenericSoundDriver::probe()
        .ok_or("No sound card found")?;

    // Initialize the driver
    driver.init()
        .map_err(|_| "Failed to initialize driver")?;
    
    // Start the driver (transitions to Active state)
    Driver::start(&mut driver)
        .map_err(|_| "Failed to start driver")?;

    // Configure audio settings: 44.1kHz, stereo, 16-bit
    let config = AudioConfig {
        sample_rate: 44100,
        channels: 2,
        format: SampleFormat::S16Le,
    };
    
    driver.set_config(config)
        .map_err(|_| "Failed to set audio config")?;

    // Start playback (AudioDevice start)
    AudioDevice::start(&mut driver)
        .map_err(|_| "Failed to start playback")?;

    // Generate a simple 440Hz sine wave (A4 note)
    let sample_rate = 44100;
    let frequency = 440.0;
    let duration_samples = sample_rate; // 1 second
    
    let mut samples = alloc::vec::Vec::new();
    
    // Generate samples (simplified - in reality would use proper sine calculation)
    for i in 0..duration_samples {
        // Simple square wave for demonstration
        let sample_value: i16 = if (i * 440 / sample_rate) % 2 == 0 {
            16000
        } else {
            -16000
        };
        
        // Write left channel
        samples.push((sample_value & 0xFF) as u8);
        samples.push(((sample_value >> 8) & 0xFF) as u8);
        
        // Write right channel (same as left for mono tone)
        samples.push((sample_value & 0xFF) as u8);
        samples.push(((sample_value >> 8) & 0xFF) as u8);
    }

    // Play the audio
    let written = driver.write(&samples)
        .map_err(|_| "Failed to write audio samples")?;

    // Wait for playback to complete (simplified)
    // In a real application, you would poll the buffer status
    
    // Stop playback (AudioDevice stop)
    AudioDevice::stop(&mut driver)
        .map_err(|_| "Failed to stop playback")?;

    Ok(())
}

/// Example: Volume control
pub fn volume_control_example() -> Result<(), &'static str> {
    let mut driver = GenericSoundDriver::probe()
        .ok_or("No sound card found")?;

    driver.init().map_err(|_| "Init failed")?;
    Driver::start(&mut driver).map_err(|_| "Start failed")?;

    // Get current volume
    let current_volume = driver.volume();
    
    // Set volume to 50%
    driver.set_volume(50)
        .map_err(|_| "Failed to set volume")?;

    // Mute audio
    driver.set_mute(true)
        .map_err(|_| "Failed to mute")?;

    // Check mute status
    if driver.is_muted() {
        // Audio is muted
    }

    // Unmute
    driver.set_mute(false)
        .map_err(|_| "Failed to unmute")?;

    Ok(())
}

/// Example: Enumerate all sound cards
pub fn enumerate_sound_cards() -> alloc::vec::Vec<GenericSoundDriver> {
    watos_driver_audio_generic::probe_all()
}

/// Example: Get device information
pub fn get_device_info() -> Result<(), &'static str> {
    let mut driver = GenericSoundDriver::probe()
        .ok_or("No sound card found")?;

    // Get driver information
    let driver_info = Driver::info(&driver);
    // driver_info.name, driver_info.version, driver_info.author, driver_info.description
    
    // Initialize driver to get audio device info
    driver.init().map_err(|_| "Init failed")?;
    Driver::start(&mut driver).map_err(|_| "Start failed")?;
    
    // Get audio device info
    let audio_device_info = AudioDevice::info(&driver);
    // audio_device_info.config, audio_device_info.buffer_size, audio_device_info.playing
    
    Ok(())
}
