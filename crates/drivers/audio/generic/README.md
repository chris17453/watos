# WATOS Generic Sound Card Driver

A software-based generic sound card driver for the WATOS operating system. This driver implements the `AudioDevice` trait and provides basic PCM audio playback support with software mixing.

## Features

- **PCI Device Detection**: Automatically detects multimedia audio devices (PCI class 0x04, subclass 0x01)
- **Flexible Audio Configuration**: Supports multiple sample rates, mono/stereo output, and various sample formats
- **Software Ring Buffer**: 16KB ring buffer for audio sample management
- **Volume Control**: Master volume control (0-100) and mute functionality
- **Standard Interface**: Implements the `AudioDevice` trait for compatibility with the WATOS audio subsystem

## Supported Audio Formats

- **Sample Rates**: Up to 96kHz (validated range)
- **Channels**: Mono (1) or Stereo (2)
- **Sample Formats**:
  - `U8`: 8-bit unsigned
  - `S16Le`: 16-bit signed little-endian (recommended)
  - `S16Be`: 16-bit signed big-endian

## Usage

### Basic Example

```rust
use watos_driver_audio_generic::GenericSoundDriver;
use watos_driver_traits::{Driver, DriverState};
use watos_driver_traits::audio::{AudioDevice, AudioConfig, SampleFormat};

// Probe for sound cards
let mut driver = GenericSoundDriver::probe()
    .expect("No sound card found");

// Initialize the driver
driver.init().expect("Failed to initialize");
driver.start().expect("Failed to start");

// Configure audio: 44.1kHz, stereo, 16-bit
let config = AudioConfig {
    sample_rate: 44100,
    channels: 2,
    format: SampleFormat::S16Le,
};
driver.set_config(config).expect("Failed to set config");

// Start playback
driver.start().expect("Failed to start playback");

// Write audio samples
let samples = vec![0u8; 4096]; // Your audio data here
let written = driver.write(&samples).expect("Write failed");

// Stop playback
driver.stop().expect("Failed to stop");
```

### Volume Control

```rust
// Get current volume (0-100)
let volume = driver.volume();

// Set volume to 75%
driver.set_volume(75).expect("Failed to set volume");

// Mute/unmute
driver.set_mute(true).expect("Failed to mute");
assert!(driver.is_muted());
```

### Enumerate All Sound Cards

```rust
use watos_driver_audio_generic;

let drivers = watos_driver_audio_generic::probe_all();
for driver in drivers {
    let info = driver.info();
    println!("Found: {}", info.name);
}
```

## Architecture

The driver consists of:

1. **PCI Enumeration**: Scans the PCI bus for multimedia audio devices
2. **Ring Buffer Management**: Software-based circular buffer for sample storage
3. **Configuration Validation**: Ensures audio parameters are within supported ranges
4. **Driver Lifecycle**: Implements init, start, stop state transitions

## Buffer Management

The driver uses a 16KB ring buffer to manage audio samples:

- **Available Space**: `driver.available()` returns bytes available for writing
- **Non-blocking Writes**: `write()` returns the number of bytes actually written
- **Automatic Wraparound**: Ring buffer handles wraparound automatically

## Integration

To use this driver in your kernel:

1. Add dependency in `Cargo.toml`:
```toml
watos-driver-audio-generic = { path = "crates/drivers/audio/generic" }
```

2. Probe and initialize at boot:
```rust
if let Some(mut audio) = GenericSoundDriver::probe() {
    audio.init().ok();
    audio.start().ok();
    // Store audio driver for later use
}
```

## Debug Features

Enable debug output at compile time:

```toml
[dependencies]
watos-driver-audio-generic = { path = "...", features = ["debug"] }
```

This enables the `debug-audio` feature from `watos-driver-traits`.

## Limitations

This is a generic software-based driver and has the following limitations:

- No hardware DMA support (all transfers are software-based)
- No hardware interrupt handling (polling-based)
- Limited to basic PCM playback (no recording, MIDI, or advanced features)
- Performance depends on CPU availability

For production use with real hardware, consider implementing hardware-specific drivers like:
- `watos-driver-ac97` for AC'97 audio
- `watos-driver-hda` for Intel High Definition Audio

## Testing

The driver includes unit tests for core functionality:

```bash
cargo test -p watos-driver-audio-generic
```

Tests cover:
- Buffer operations (read/write/wraparound)
- Configuration validation
- Volume control
- Mute functionality

## License

Part of the WATOS project.
