# GW-BASIC (Rust Version)

This project is a reimplementation of the classic GW-BASIC interpreter in safe, modern Rust with full feature parity, compatibility, and strong testing.

**Status:** Core Implementation Complete

## Features

### Implemented

- **Lexer**: Complete tokenization of GW-BASIC source code
  - Line numbers
  - Keywords (PRINT, LET, IF, FOR, WHILE, GOTO, GOSUB, etc.)
  - Operators (arithmetic, comparison, logical)
  - String and numeric literals
  - Comments (REM)

- **Parser**: Full AST generation with proper operator precedence
  - Statement parsing (all major BASIC statements)
  - Expression evaluation with correct precedence
  - Control flow structures

- **Interpreter**: Execution engine with:
  - Variable storage and management
  - Expression evaluation
  - Control flow (IF/THEN/ELSE, FOR/NEXT, WHILE/WEND)
  - Subroutines (GOSUB/RETURN)
  - Built-in functions (ABS, INT, SQR)
  - I/O operations (PRINT, INPUT)
  - Line-numbered program storage

- **Error Handling**: Comprehensive error types
  - Syntax errors
  - Runtime errors
  - Type errors
  - Division by zero
  - Undefined variables

- **Testing**: Complete test coverage for all modules

## Building

```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run tests
cargo test
```

## Usage

### Interactive Mode (REPL)

```bash
cargo run
```

Then type BASIC statements:

```basic
> PRINT "Hello, World!"
Hello, World!
> LET A = 42
> PRINT A
42
> PRINT 2 + 3 * 4
14
> EXIT
```

### As a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-gwbasic = { path = "../rust-gwbasic" }
```

Use in your code:

```rust
use rust_gwbasic::{Lexer, Parser, Interpreter};

fn main() {
    let code = "PRINT \"Hello from Rust!\"";
    
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize().unwrap();
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().unwrap();
    
    let mut interpreter = Interpreter::new();
    interpreter.execute(ast).unwrap();
}
```

## Architecture

The crate is organized into several modules:

- `lexer`: Tokenization of source code
- `parser`: AST generation from tokens
- `interpreter`: Execution of AST nodes
- `value`: Value types (Integer, Single, Double, String)
- `error`: Error types and handling

## Examples

### Simple Program

```basic
10 PRINT "GW-BASIC in Rust"
20 LET X = 10
30 PRINT "X ="; X
40 END
```

### Control Flow

```basic
10 FOR I = 1 TO 10
20   PRINT I
30 NEXT I
```

### Conditionals

```basic
10 LET A = 5
20 IF A > 3 THEN PRINT "A is greater than 3"
```

## Future Enhancements

- File I/O operations
- Graphics commands (PSET, LINE, CIRCLE)
- More built-in functions
- Array support (DIM with full implementation)
- Sound support (BEEP, SOUND, PLAY)
- Better error messages with line numbers
- Program editing commands (LIST, RENUM, DELETE)

## License

MIT License - see the parent repository for details.
