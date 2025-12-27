# GW-BASIC Rust Interpreter - Example Programs

## Simple Hello World

```basic
PRINT "Hello, World!"
```

## Variables and Arithmetic

```basic
LET A = 10
LET B = 20
PRINT "A ="; A
PRINT "B ="; B
LET C = A + B
PRINT "A + B ="; C
```

## Control Flow - IF Statement

```basic
LET X = 15
IF X > 10 THEN PRINT "X is greater than 10"
IF X < 10 THEN PRINT "X is less than 10" ELSE PRINT "X is 10 or greater"
```

## Loops - FOR/NEXT

```basic
10 FOR I = 1 TO 10
20   PRINT "Number:"; I
30 NEXT I
```

## Expression Evaluation

```basic
PRINT 2 + 3 * 4
PRINT (2 + 3) * 4
PRINT 2 ^ 3
PRINT 10 / 3
```

## Built-in Functions

```basic
PRINT "ABS(-5) ="; ABS(-5)
PRINT "INT(3.7) ="; INT(3.7)
PRINT "SQR(16) ="; SQR(16)
```

## String Operations

```basic
LET NAME$ = "World"
PRINT "Hello, "; NAME$; "!"
```

## Line-Numbered Programs

```basic
10 PRINT "GW-BASIC Program"
20 LET X = 100
30 PRINT "X ="; X
40 END
```

## Comparison Operations

```basic
LET A = 5
LET B = 3
IF A = B THEN PRINT "Equal"
IF A <> B THEN PRINT "Not equal"
IF A > B THEN PRINT "A greater than B"
IF A < B THEN PRINT "A less than B"
```

## Running Programs

### Interactive Mode (REPL)

```bash
cargo run
```

Then type statements interactively:

```
> PRINT "Hello!"
Hello!
> LET X = 42
> PRINT X
42
```

### Programmatic Use

```rust
use rust_gwbasic::{Lexer, Parser, Interpreter};

fn main() {
    let code = r#"
        PRINT "Computing..."
        LET RESULT = 2 + 2
        PRINT "2 + 2 ="; RESULT
    "#;

    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize().unwrap();
    
    let mut parser = Parser::new(tokens);
    let ast = parser.parse().unwrap();
    
    let mut interpreter = Interpreter::new();
    interpreter.execute(ast).unwrap();
}
```

## Notes

- The interpreter supports both immediate mode (no line numbers) and program mode (with line numbers)
- Statements can be separated by newlines or colons
- Keywords are case-insensitive (PRINT, print, Print all work)
- Variable names can include $ (strings) or % (integers) suffixes
- Operator precedence follows standard mathematical rules
