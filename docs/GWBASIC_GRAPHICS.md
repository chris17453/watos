# GW-BASIC Graphics Integration with SVGA Driver

## Overview

GW-BASIC has been updated to use the WATOS SVGA driver for high-resolution graphics modes. This integration provides:

- **Text Mode** (SCREEN 0): 80x25 character display
- **VGA Modes**: 320x200, 640x200, 640x480 with 256 or 16 colors
- **SVGA Modes**: 800x600 and 1024x768 with 32-bit true color

## Architecture

### Graphics Backend System

The GW-BASIC interpreter uses a trait-based graphics backend system:

```rust
pub trait GraphicsBackend {
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()>;
    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) -> Result<()>;
    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u8) -> Result<()>;
    fn cls(&mut self);
    // ... more methods
}
```

Available backends:
- **AsciiBackend**: Text-based rendering (always available)
- **WindowBackend**: GUI window for host systems (std/host only)
- **WatosVgaBackend**: Hardware graphics via SVGA driver (watos/no_std only)

### SVGA Session Management

The `WatosVgaBackend` uses the kernel's VGA session management syscalls:

- **SYS_VGA_CREATE_SESSION (37)**: Create a virtual framebuffer with specified resolution
- **SYS_VGA_DESTROY_SESSION (38)**: Clean up a session
- **SYS_VGA_SET_ACTIVE_SESSION (39)**: Switch which session is displayed
- **SYS_VGA_BLIT (33)**: Copy pixel data from app to kernel
- **SYS_VGA_FLIP (35)**: Composite session to physical display

Each GW-BASIC graphics screen mode creates its own VGA session, providing isolated framebuffers and proper cleanup when switching modes.

## SCREEN Modes

| Mode | Resolution | BPP | Description |
|------|-----------|-----|-------------|
| 0 | 80x25 | text | Text mode (ASCII backend) |
| 1 | 320x200 | 8 | VGA low-res, 256 indexed colors |
| 2 | 640x200 | 4 | VGA medium-res, 16 indexed colors |
| 3 | 640x480 | 4 | VGA high-res, 16 indexed colors |
| 4 | 800x600 | 32 | SVGA 800x600, 32-bit RGBA with color index mapping |
| 5 | 1024x768 | 32 | SVGA 1024x768, 32-bit RGBA with color index mapping |

**Note on SVGA Color Modes**: While SVGA modes use 32-bit RGBA framebuffers internally for better driver compatibility, GW-BASIC programs still use 8-bit color indices (0-255) in PSET, LINE, and CIRCLE commands. The backend automatically maps these indices to RGB values using the EGA 16-color palette for indices 0-15, and grayscale for higher indices.

## Graphics Commands

### SCREEN
```basic
SCREEN 4  ' Switch to 800x600 SVGA mode
```

### PSET - Set Pixel
```basic
PSET (100, 200), 15  ' Set pixel at (100, 200) to white
```

### LINE - Draw Line
```basic
LINE (0, 0)-(799, 599), 14  ' Draw yellow line from corner to corner
```

### CIRCLE - Draw Circle
```basic
CIRCLE (400, 300), 100, 12  ' Draw red circle at center with radius 100
```

### CLS - Clear Screen
```basic
CLS  ' Clear the screen with background color
```

### COLOR - Set Colors
```basic
COLOR 15, 0  ' Set foreground to white, background to black
```

## Example Programs

### Simple SVGA Test
See `examples/svga_test.bas` - draws colored rectangles, circles, and lines in 800x600 mode.

### Mandelbrot Set
See `examples/mandelbrot_svga.bas` - renders the Mandelbrot fractal in high resolution.

```basic
10 REM Mandelbrot in SVGA
20 SCREEN 4
30 CLS
40 FOR Y = 0 TO 599
50   FOR X = 0 TO 799
60     REM Calculate Mandelbrot iterations
70     REM ... (see full example)
80     PSET (X, Y), COLOR
90   NEXT X
100 NEXT Y
```

## Implementation Details

### Mode Switching

When `SCREEN n` is called:
1. Interpreter evaluates the mode number
2. Determines appropriate resolution and backend
3. For graphics modes (n > 0):
   - Creates `WatosVgaBackend` with session
   - Allocates local framebuffer
   - Registers VGA session with kernel
4. For text mode (n = 0):
   - Uses `AsciiBackend` (no VGA session)

### Pixel Operations

Graphics operations use a double-buffering approach:
1. App maintains local framebuffer (Vec<u8>)
2. Graphics commands update local buffer
3. `display()` or `update()` blits to kernel via SYS_VGA_BLIT
4. Kernel composites to physical display via SYS_VGA_FLIP

### Cleanup

The `Drop` trait ensures VGA sessions are properly destroyed:
```rust
impl Drop for WatosVgaBackend {
    fn drop(&mut self) {
        if let Some(session_id) = self.session_id {
            unsafe {
                syscall1(SYS_VGA_DESTROY_SESSION, session_id as u64);
            }
        }
    }
}
```

## Testing

To test GW-BASIC graphics on WATOS:

1. Build the system: `./scripts/build.sh`
2. Boot in QEMU: `./scripts/boot_test.sh -i`
3. Launch GW-BASIC: `GWBASIC`
4. Run a graphics program:
   ```basic
   SCREEN 4
   FOR I = 0 TO 15
     LINE (I*50, 100)-(I*50+40, 200), I
   NEXT I
   ```

## Known Limitations

1. Color mapping from 8-bit indices to 32-bit RGBA may need adjustment
2. SVGA modes use true color (32 bpp) rather than indexed color (8 bpp)
3. Some legacy VGA color indices may not map correctly
4. Performance in interpreted BASIC limits practical resolution for complex graphics

## Future Enhancements

- [ ] Support for PAINT (flood fill)
- [ ] Support for GET/PUT (sprite capture/display)
- [ ] Optimize blitting for large framebuffers
- [ ] Add support for custom palettes in SVGA modes
- [ ] Hardware acceleration for LINE and CIRCLE primitives
