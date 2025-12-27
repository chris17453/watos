# GW-BASIC Example Programs - Final Status

## ✅ FULLY WORKING EXAMPLES (15/19)

### Basic Programs
- **hello.bas** - Hello World output
- **arithmetic.bas** - All arithmetic operators (+, -, *, /, \, MOD, ^)
- **loops.bas** - FOR/NEXT loops with STEP, nested loops
- **conditionals.bas** - IF/THEN/ELSE statements
- **strings.bas** - String operations (LEN, LEFT$, RIGHT$, MID$)
- **functions.bas** - Math functions (ABS, INT, SQR, SIN, COS, TAN, LOG, EXP, RND)
- **subroutines.bas** - GOSUB/RETURN subroutines ✨ FIXED!

### Graphics Programs  
- **flower.bas** - Single flower with petals and stem ✨ FIXED!
- **flowers.bas** - Garden with multiple flowers ✨ FIXED!
- **spirograph.bas** - Geometric spirograph pattern ✨ FIXED!
- **fractal-tree.bas** - Recursive fractal tree ✨ FIXED!
- **flower_simple.bas** - Simplified flower (no cleanup)
- **spirograph_simple.bas** - Simplified spirograph
- **mandelbrot_simple.bas** - Fast Mandelbrot render

### Slow But Working
- **mandelbrot.bas** - Full Mandelbrot (takes ~60+ seconds)

## ❌ NOT YET WORKING (4/19)

### Parser/Feature Limitations
- **arrays.bas** - Needs array assignment: `LET ARR(I) = value`
- **fibonacci.bas** - Needs INPUT with prompts: `INPUT "text"; var`
- **guess.bas** - Needs RANDOMIZE TIMER and INPUT prompts
- **mandelbrot-hires.bas** - Needs INPUT with prompts

## Running the Examples

```bash
# Basic examples
cargo run examples/hello.bas
cargo run examples/arithmetic.bas
cargo run examples/loops.bas
cargo run examples/functions.bas
cargo run examples/subroutines.bas

# Graphics (320x200 - very wide, use less -S or redirect)
cargo run examples/flower.bas | less -S
cargo run examples/spirograph.bas | less -S
cargo run examples/fractal-tree.bas | less -S

# Graphics (simplified versions)
cargo run examples/flower_simple.bas | less -S
cargo run examples/spirograph_simple.bas | less -S
cargo run examples/mandelbrot_simple.bas | less -S
```

## Recent Fixes

1. ✅ **GOSUB/RETURN infinite loop** - Fixed! Returns to correct line
2. ✅ **INPUT$ errors** - Fixed! Works in non-interactive mode
3. ✅ **Graphics rendering** - Working! PSET, LINE, CIRCLE all render
4. ✅ **SCREEN mode** - Fixed! Properly sets 320x200 or 640x200 buffers
5. ✅ **FOR/NEXT loops** - Fixed! Properly loop back and iterate

## Success Rate

**15 out of 19 programs working = 79% success rate!**

