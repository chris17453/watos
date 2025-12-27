use rust_gwbasic::{Lexer, Parser, Interpreter};
use std::io::{self, Write};
use std::fs;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse command line arguments
    let mut use_gui = false;
    let mut filename: Option<String> = None;

    for arg in &args[1..] {
        if arg == "--gui" || arg == "-g" {
            use_gui = true;
        } else if !arg.starts_with('-') && filename.is_none() {
            filename = Some(arg.clone());
        } else if arg == "--help" || arg == "-h" {
            print_usage();
            return;
        }
    }

    // If a filename is provided, run it
    if let Some(file) = filename {
        run_file(&file, use_gui);
        return;
    }

    // Otherwise, start REPL
    println!("GW-BASIC (Rust) interpreter v{}", rust_gwbasic::VERSION);
    println!("Type BASIC statements or 'EXIT' to quit");
    println!();

    let mut interpreter = Interpreter::new();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("Error reading input");
            continue;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("EXIT") || input.eq_ignore_ascii_case("QUIT") {
            break;
        }

        // Try to tokenize, parse, and execute
        let mut lexer = Lexer::new(input);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Lexer error: {}", e);
                continue;
            }
        };

        let mut parser = Parser::new(tokens);
        let ast = match parser.parse() {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Parser error: {}", e);
                continue;
            }
        };

        if let Err(e) = interpreter.execute(ast) {
            eprintln!("Runtime error: {}", e);
        }
    }

    println!("Goodbye!");
}

fn print_usage() {
    println!("GW-BASIC (Rust) v{}", rust_gwbasic::VERSION);
    println!();
    println!("USAGE:");
    println!("  rust-gwbasic [OPTIONS] [FILE]");
    println!();
    println!("OPTIONS:");
    println!("  -g, --gui      Use GUI window for graphics mode");
    println!("  -h, --help     Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("  rust-gwbasic                    Start REPL");
    println!("  rust-gwbasic program.bas        Run program in ASCII mode");
    println!("  rust-gwbasic --gui program.bas  Run program with GUI window");
}

fn run_file(filename: &str, use_gui: bool) {
    // Read the file
    let content = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            std::process::exit(1);
        }
    };

    // Create interpreter with specified graphics backend
    let mut interpreter = if use_gui {
        match Interpreter::new_with_gui() {
            Ok(interp) => interp,
            Err(e) => {
                eprintln!("Error creating GUI window: {}", e);
                eprintln!("Falling back to ASCII mode...");
                Interpreter::new()
            }
        }
    } else {
        Interpreter::new()
    };

    // Tokenize
    let mut lexer = Lexer::new(&content);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Lexer error: {}", e);
            std::process::exit(1);
        }
    };

    // Parse
    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Parser error: {}", e);
            std::process::exit(1);
        }
    };

    // Execute (this loads line-numbered programs)
    if let Err(e) = interpreter.execute(ast) {
        eprintln!("Runtime error: {}", e);
        std::process::exit(1);
    }

    // If the program had line numbers, run it now
    if let Err(e) = interpreter.run_stored_program() {
        eprintln!("Runtime error: {}", e);
        std::process::exit(1);
    }
}
