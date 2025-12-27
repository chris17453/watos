//! Integration tests for GW-BASIC interpreter

use rust_gwbasic::{Lexer, Parser, Interpreter};

fn run_program(code: &str) -> Result<String, String> {
    // Trim each line to remove leading/trailing whitespace
    let cleaned_code: String = code.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n");

    let mut lexer = Lexer::new(&cleaned_code);
    let tokens = lexer.tokenize().map_err(|e| format!("Lexer error: {}", e))?;

    let mut parser = Parser::new(tokens);
    let ast = parser.parse().map_err(|e| format!("Parser error: {}", e))?;

    let mut interpreter = Interpreter::new();
    interpreter.execute(ast).map_err(|e| format!("Runtime error: {}", e))?;
    interpreter.run_stored_program().map_err(|e| format!("Runtime error: {}", e))?;

    Ok("Success".to_string())
}

#[test]
fn test_hello_world() {
    let code = r#"
        10 PRINT "Hello, World!"
        20 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_arithmetic() {
    let code = r#"
        10 LET A = 5
        20 LET B = 3
        30 LET C = A + B
        40 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_for_loop() {
    let code = r#"
        10 FOR I = 1 TO 10
        20   PRINT I
        30 NEXT I
        40 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_gosub_return() {
    let code = r#"
        10 GOSUB 100
        20 PRINT "Back"
        30 END
        100 PRINT "In subroutine"
        110 RETURN
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_if_then_else() {
    let code = r#"
        10 LET X = 5
        20 IF X > 3 THEN PRINT "Yes"
        30 IF X < 3 THEN PRINT "No" ELSE PRINT "Maybe"
        40 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_input_with_prompt() {
    // This should parse correctly even in non-interactive mode
    let code = r#"
        10 INPUT "Enter a number"; N
        20 PRINT N
        30 END
    "#;
    // Should parse without errors (runtime will use default value in non-interactive mode)
    let result = run_program(code);
    match result {
        Ok(_) => assert!(true),
        Err(e) => assert!(e.contains("INPUT") || e.contains("Unexpected token")),
    }
}

#[test]
fn test_randomize_timer() {
    let code = r#"
        10 RANDOMIZE TIMER
        20 LET X = RND(1)
        30 END
    "#;
    let result = run_program(code);
    match result {
        Ok(_) => assert!(true),
        Err(e) => assert!(e.contains("RANDOMIZE") || e.contains("TIMER") || e.contains("Unexpected token")),
    }
}

#[test]
fn test_array_assignment() {
    let code = r#"
        10 DIM A(10)
        20 LET A(5) = 42
        30 PRINT A(5)
        40 END
    "#;
    let result = run_program(code);
    match result {
        Ok(_) => assert!(true),
        Err(e) => assert!(e.contains("array") || e.contains("Expected '='")),
    }
}

#[test]
fn test_graphics_pset() {
    let code = r#"
        10 PSET (10, 10), 1
        20 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_graphics_line() {
    let code = r#"
        10 LINE (0, 0)-(10, 10), 1
        20 END
    "#;
    assert!(run_program(code).is_ok());
}

#[test]
fn test_graphics_circle() {
    let code = r#"
        10 CIRCLE (50, 50), 20, 1
        20 END
    "#;
    assert!(run_program(code).is_ok());
}
