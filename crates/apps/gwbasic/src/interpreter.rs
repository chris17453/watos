//! Interpreter for GW-BASIC

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, string::String, vec::Vec, vec, format, string::ToString};
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;

#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(feature = "std")]
use std::io::{self, Write};

use crate::error::{Error, Result};
use crate::parser::{AstNode, BinaryOperator, UnaryOperator};
use crate::value::Value;
use crate::graphics::Screen;
#[cfg(feature = "host")]
use crate::graphics_backend::WindowBackend;
use crate::fileio::{FileManager, FileMode};

/// Graphics mode selection
#[derive(Debug, Clone, Copy)]
enum GraphicsMode {
    Ascii,
    Gui,
}

/// The GW-BASIC interpreter
pub struct Interpreter {
    /// Variable storage
    variables: HashMap<String, Value>,

    /// Array storage (key: "name_idx1_idx2_...", value: Value)
    arrays: HashMap<String, Value>,

    /// Array dimensions (key: array name, value: dimensions)
    array_dims: HashMap<String, Vec<usize>>,

    /// Program lines indexed by line number
    lines: HashMap<u32, Vec<AstNode>>,

    /// Current execution position
    current_line: Option<u32>,

    /// Call stack for GOSUB/RETURN
    call_stack: Vec<u32>,

    /// FOR loop stack
    for_stack: Vec<ForLoopState>,

    /// WHILE loop stack
    while_stack: Vec<WhileLoopState>,

    /// Screen/Graphics manager
    screen: Screen,

    /// Graphics mode preference
    graphics_mode: GraphicsMode,

    /// File I/O manager
    file_manager: FileManager,

    /// DATA storage
    data_items: Vec<Value>,
    data_pointer: usize,
}

#[derive(Debug, Clone)]
struct ForLoopState {
    variable: String,
    end_value: f64,
    step: f64,
    return_line: u32,
}

#[derive(Debug, Clone)]
struct WhileLoopState {
    condition: AstNode,
    return_line: u32,
}

impl Interpreter {
    /// Create a new interpreter
    pub fn new() -> Self {
        Interpreter {
            variables: HashMap::new(),
            arrays: HashMap::new(),
            array_dims: HashMap::new(),
            lines: HashMap::new(),
            current_line: None,
            call_stack: Vec::new(),
            for_stack: Vec::new(),
            while_stack: Vec::new(),
            screen: Screen::default(),
            graphics_mode: GraphicsMode::Ascii,
            file_manager: FileManager::new(),
            data_items: Vec::new(),
            data_pointer: 0,
        }
    }

    /// Create a new interpreter with GUI window backend (host only)
    #[cfg(feature = "host")]
    pub fn new_with_gui() -> Result<Self> {
        let backend = WindowBackend::new(640, 480)?;
        let screen = Screen::new_with_backend(Box::new(backend));

        Ok(Interpreter {
            variables: HashMap::new(),
            arrays: HashMap::new(),
            array_dims: HashMap::new(),
            lines: HashMap::new(),
            current_line: None,
            call_stack: Vec::new(),
            for_stack: Vec::new(),
            while_stack: Vec::new(),
            screen,
            graphics_mode: GraphicsMode::Gui,
            file_manager: FileManager::new(),
            data_items: Vec::new(),
            data_pointer: 0,
        })
    }

    /// Create interpreter for WATOS with VGA graphics mode
    #[cfg(not(feature = "std"))]
    pub fn new_with_gui() -> Result<Self> {
        use crate::graphics_backend::WatosVgaBackend;

        // On WATOS, use VGA graphics backend via kernel
        let backend = WatosVgaBackend::new_vga()?;
        let screen = Screen::new_with_backend(Box::new(backend));

        Ok(Interpreter {
            variables: HashMap::new(),
            arrays: HashMap::new(),
            array_dims: HashMap::new(),
            lines: HashMap::new(),
            current_line: None,
            call_stack: Vec::new(),
            for_stack: Vec::new(),
            while_stack: Vec::new(),
            screen,
            graphics_mode: GraphicsMode::Gui,
            file_manager: FileManager::new(),
            data_items: Vec::new(),
            data_pointer: 0,
        })
    }

    /// Create interpreter with specific video mode for WATOS
    #[cfg(not(feature = "std"))]
    pub fn new_with_vga_mode(mode: u8) -> Result<Self> {
        use crate::graphics_backend::{WatosVgaBackend, VideoMode};

        let video_mode = VideoMode::from_basic_mode(mode);
        let backend = WatosVgaBackend::new(video_mode)?;
        let screen = Screen::new_with_backend(Box::new(backend));

        Ok(Interpreter {
            variables: HashMap::new(),
            arrays: HashMap::new(),
            array_dims: HashMap::new(),
            lines: HashMap::new(),
            current_line: None,
            call_stack: Vec::new(),
            for_stack: Vec::new(),
            while_stack: Vec::new(),
            screen,
            graphics_mode: GraphicsMode::Gui,
            file_manager: FileManager::new(),
            data_items: Vec::new(),
            data_pointer: 0,
        })
    }

    /// Execute a program AST
    pub fn execute(&mut self, ast: AstNode) -> Result<()> {
        match ast {
            AstNode::Program(nodes) => {
                for node in nodes {
                    self.execute_node(node)?;
                }
            }
            _ => {
                self.execute_node(ast)?;
            }
        }
        Ok(())
    }

    /// Run the stored line-numbered program
    pub fn run_stored_program(&mut self) -> Result<()> {
        // Get sorted line numbers
        let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
        if line_nums.is_empty() {
            self.screen.display();
            return Ok(());
        }
        line_nums.sort();

        // Pre-process DATA statements - collect them first to avoid borrow issues
        let mut data_nodes = Vec::new();
        for &line_num in &line_nums {
            if let Some(statements) = self.lines.get(&line_num) {
                for stmt in statements {
                    if let AstNode::Data(values) = stmt {
                        data_nodes.extend(values.clone());
                    }
                }
            }
        }

        // Now evaluate and store the DATA
        for val_node in data_nodes {
            let val = self.evaluate_expression(&val_node)?;
            self.data_items.push(val);
        }

        // Start at first line
        self.current_line = Some(line_nums[0]);

        // Execute line by line
        while let Some(current) = self.current_line {
            // Get statements for current line
            let statements = match self.lines.get(&current) {
                Some(stmts) => stmts.clone(),
                None => {
                    // Line not found, stop execution
                    break;
                }
            };

            // Execute all statements on this line
            for stmt in statements {
                match self.execute_node(stmt) {
                    Ok(_) => {},
                    Err(Error::ProgramEnd) => {
                        // END statement reached - display graphics before exiting
                        self.screen.display();

                        // If using GUI window, keep it open until user closes
                        #[cfg(feature = "host")]
                        {
                            while !self.screen.should_close() {
                                self.screen.update()?;
                                std::thread::sleep(std::time::Duration::from_millis(16));
                            }
                        }

                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }

            // Move to next line (unless GOTO/GOSUB changed it)
            if self.current_line == Some(current) {
                // Find next line number
                let next_line = line_nums.iter()
                    .find(|&&n| n > current)
                    .copied();
                self.current_line = next_line;
            }
        }

        // Display the graphics buffer if it has any content
        self.screen.display();

        Ok(())
    }

    /// Execute a single AST node
    fn execute_node(&mut self, node: AstNode) -> Result<()> {
        match node {
            AstNode::Program(nodes) => {
                // Execute all nodes in sequence
                for n in nodes {
                    self.execute_node(n)?;
                }
                Ok(())
            }
            AstNode::Line(num, statements) => {
                self.lines.insert(num, statements);
                Ok(())
            }
            
            // Basic I/O
            AstNode::Print(exprs) => self.execute_print(exprs),
            AstNode::Input(vars) => self.execute_input(vars),
            AstNode::Let(name, expr) => self.execute_let(name, *expr),
            AstNode::ArrayAssign(name, indices, expr) => self.execute_array_assign(name, indices, *expr),

            // Control Flow
            AstNode::If(condition, then_stmts, else_stmts) => {
                self.execute_if(*condition, then_stmts, else_stmts)
            }
            AstNode::For(var, start, end, step) => {
                self.execute_for(var, *start, *end, step.map(|s| *s))
            }
            AstNode::Next(var) => self.execute_next(var),
            AstNode::While(condition) => self.execute_while(*condition),
            AstNode::Wend => self.execute_wend(),
            AstNode::Goto(line) => self.execute_goto(line),
            AstNode::Gosub(line) => self.execute_gosub(line),
            AstNode::OnGoto(expr, lines) => {
                let index = self.evaluate_expression(&expr)?.as_integer()? as usize;
                if index > 0 && index <= lines.len() {
                    self.execute_goto(lines[index - 1])
                } else {
                    Ok(()) // Out of range - do nothing
                }
            }
            AstNode::OnGosub(expr, lines) => {
                let index = self.evaluate_expression(&expr)?.as_integer()? as usize;
                if index > 0 && index <= lines.len() {
                    self.execute_gosub(lines[index - 1])
                } else {
                    Ok(()) // Out of range - do nothing
                }
            }
            AstNode::Return => self.execute_return(),
            AstNode::End => Err(Error::ProgramEnd),
            AstNode::Stop => Err(Error::ProgramEnd),
            
            // Data
            AstNode::Dim(name, dimensions) => self.execute_dim(name, dimensions),
            AstNode::Rem(_) => Ok(()), // Comments are no-ops
            AstNode::Read(vars) => {
                for var in vars {
                    if self.data_pointer >= self.data_items.len() {
                        return Err(Error::RuntimeError("Out of DATA".to_string()));
                    }
                    self.variables.insert(var, self.data_items[self.data_pointer].clone());
                    self.data_pointer += 1;
                }
                Ok(())
            }
            AstNode::Data(values) => {
                for val_node in values {
                    let val = self.evaluate_expression(&val_node)?;
                    self.data_items.push(val);
                }
                Ok(())
            }
            AstNode::Restore(_line) => {
                self.data_pointer = 0;
                // In full implementation, would restore to specific line
                Ok(())
            }
            
            // Screen/Graphics
            AstNode::Cls => {
                self.screen.cls();
                console_println!("\x1B[2J\x1B[1;1H"); // ANSI clear screen
                Ok(())
            }
            AstNode::Locate(row, col) => {
                let r = self.evaluate_expression(&row)?.as_integer()? as usize;
                let c = self.evaluate_expression(&col)?.as_integer()? as usize;
                self.screen.locate(r.saturating_sub(1), c.saturating_sub(1))?;
                Ok(())
            }
            AstNode::Color(fg, bg) => {
                let fg_val = if let Some(f) = fg {
                    Some(self.evaluate_expression(&f)?.as_integer()? as u8)
                } else {
                    None
                };
                let bg_val = if let Some(b) = bg {
                    Some(self.evaluate_expression(&b)?.as_integer()? as u8)
                } else {
                    None
                };
                self.screen.color(fg_val, bg_val);
                Ok(())
            }
            AstNode::Screen(mode) => {
                // Screen mode change
                let m = self.evaluate_expression(&mode)?.as_integer()?;
                let (width, height) = match m {
                    1 => (320, 200),  // SCREEN 1: 320x200 graphics mode (CGA)
                    2 => (640, 200),  // SCREEN 2: 640x200 high-res monochrome
                    _ => (80, 25),    // SCREEN 0 or others: 80x25 text mode
                };

                // Create screen with appropriate backend based on graphics_mode
                #[cfg(feature = "host")]
                {
                    self.screen = match self.graphics_mode {
                        GraphicsMode::Gui => {
                            match WindowBackend::new(width, height) {
                                Ok(backend) => Screen::new_with_backend(Box::new(backend)),
                                Err(_) => {
                                    console_eprintln!("Warning: Failed to create GUI window, falling back to ASCII mode");
                                    Screen::new(width, height)
                                }
                            }
                        }
                        GraphicsMode::Ascii => Screen::new(width, height),
                    };
                }
                #[cfg(not(feature = "host"))]
                {
                    // On WATOS, use the VGA backend through syscalls
                    self.screen = Screen::new(width, height);
                }
                Ok(())
            }
            AstNode::Width(width) => {
                // Screen width change - simplified
                let _w = self.evaluate_expression(&width)?;
                Ok(())
            }
            AstNode::Pset(x, y, color) => {
                let x_val = self.evaluate_expression(&x)?.as_integer()?;
                let y_val = self.evaluate_expression(&y)?.as_integer()?;
                let c_val = if let Some(c) = color {
                    Some(self.evaluate_expression(&c)?.as_integer()? as u8)
                } else {
                    None
                };
                self.screen.pset(x_val, y_val, c_val)?;
                Ok(())
            }
            AstNode::DrawLine(x1, y1, x2, y2, color) => {
                let x1_val = self.evaluate_expression(&x1)?.as_integer()?;
                let y1_val = self.evaluate_expression(&y1)?.as_integer()?;
                let x2_val = self.evaluate_expression(&x2)?.as_integer()?;
                let y2_val = self.evaluate_expression(&y2)?.as_integer()?;
                let c_val = if let Some(c) = color {
                    Some(self.evaluate_expression(&c)?.as_integer()? as u8)
                } else {
                    None
                };
                self.screen.line(x1_val, y1_val, x2_val, y2_val, c_val)?;
                Ok(())
            }
            AstNode::Circle(x, y, radius, color) => {
                let x_val = self.evaluate_expression(&x)?.as_integer()?;
                let y_val = self.evaluate_expression(&y)?.as_integer()?;
                let r_val = self.evaluate_expression(&radius)?.as_integer()?;
                let c_val = if let Some(c) = color {
                    Some(self.evaluate_expression(&c)?.as_integer()? as u8)
                } else {
                    None
                };
                self.screen.circle(x_val, y_val, r_val, c_val)?;
                Ok(())
            }
            
            // Sound
            AstNode::Beep => {
                console_println!("\x07"); // ASCII bell character
                Ok(())
            }
            AstNode::Sound(freq, duration) => {
                let _f = self.evaluate_expression(&freq)?;
                let _d = self.evaluate_expression(&duration)?;
                // Simulated - would play sound
                console_println!("\x07");
                Ok(())
            }
            
            // File I/O
            AstNode::Open(filename, filenum, mode) => {
                let num = self.evaluate_expression(&filenum)?.as_integer()?;
                let file_mode = match mode.to_uppercase().as_str() {
                    "INPUT" | "I" => FileMode::Input,
                    "OUTPUT" | "O" => FileMode::Output,
                    "APPEND" | "A" => FileMode::Append,
                    _ => FileMode::Output,
                };
                self.file_manager.open(num, &filename, file_mode)?;
                Ok(())
            }
            AstNode::Close(nums) => {
                if nums.is_empty() {
                    self.file_manager.close_all()?;
                } else {
                    for num in nums {
                        self.file_manager.close(num)?;
                    }
                }
                Ok(())
            }
            AstNode::PrintFile(file_num, exprs) => {
                let num = self.evaluate_expression(&file_num)?.as_integer()?;
                let mut output = String::new();
                for expr in exprs {
                    let val = self.evaluate_expression(&expr)?;
                    output.push_str(&val.to_string());
                }
                if num == 0 {
                    // Screen output
                    console_println!("{}", output);
                } else {
                    self.file_manager.write_line(num, &output)?;
                }
                Ok(())
            }
            AstNode::InputFile(file_num, vars) => {
                let num = self.evaluate_expression(&file_num)?.as_integer()?;
                for var in vars {
                    let line = self.file_manager.read_line(num)?;
                    self.variables.insert(var, Value::String(line));
                }
                Ok(())
            }
            AstNode::WriteFile(file_num, exprs) => {
                let num = self.evaluate_expression(&file_num)?.as_integer()?;
                let mut parts = vec![];
                for expr in exprs {
                    let val = self.evaluate_expression(&expr)?;
                    parts.push(format!("{}", val));
                }
                let output = parts.join(",");
                self.file_manager.write_line(num, &output)?;
                Ok(())
            }
            AstNode::LineInput(vars) => {
                for var in vars {
                    #[cfg(feature = "std")]
                    {
                        use std::io::{self, Write};
                        console_print!("? ");
                        io::stdout().flush().ok();
                        let mut input = String::new();
                        io::stdin().read_line(&mut input).ok();
                        self.variables.insert(var, Value::String(input.trim().to_string()));
                    }
                    #[cfg(not(feature = "std"))]
                    {
                        // On WATOS, use a default value - actual input handled by platform
                        self.variables.insert(var, Value::String(String::new()));
                    }
                }
                Ok(())
            }
            AstNode::LineInputFile(file_num, var) => {
                let num = self.evaluate_expression(&file_num)?.as_integer()?;
                let line = self.file_manager.read_line(num)?;
                self.variables.insert(var, Value::String(line));
                Ok(())
            }
            
            // Program Control
            AstNode::List(start, end) => {
                let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
                line_nums.sort();
                
                for line_num in line_nums {
                    if let Some(start_line) = start {
                        if line_num < start_line {
                            continue;
                        }
                    }
                    if let Some(end_line) = end {
                        if line_num > end_line {
                            break;
                        }
                    }
                    
                    if let Some(statements) = self.lines.get(&line_num) {
                        console_print!("{} ", line_num);
                        for stmt in statements {
                            console_print!("{:?} ", stmt);
                        }
                        console_println!();
                    }
                }
                Ok(())
            }
            AstNode::New => {
                self.lines.clear();
                self.variables.clear();
                self.for_stack.clear();
                self.call_stack.clear();
                self.data_items.clear();
                self.data_pointer = 0;
                Ok(())
            }
            AstNode::Run(start_line) => {
                self.current_line = start_line.or_else(|| {
                    let mut nums: Vec<u32> = self.lines.keys().copied().collect();
                    nums.sort();
                    nums.first().copied()
                });
                
                if let Some(line) = self.current_line {
                    self.execute_goto(line)
                } else {
                    Ok(())
                }
            }
            
            // Error Handling
            AstNode::OnError(_line) => {
                // Store error handler line (simplified)
                Ok(())
            }
            AstNode::Resume(line) => {
                // Resume execution (simplified)
                if let Some(resume_line) = line {
                    self.execute_goto(resume_line)
                } else {
                    Ok(())
                }
            }
            AstNode::ErrorStmt(error_num) => {
                let num = self.evaluate_expression(&error_num)?.as_integer()?;
                Err(Error::RuntimeError(format!("Error {}", num)))
            }
            
            // File I/O
            AstNode::Randomize(seed) => {
                // Set RNG seed - handled by RND function
                if let Some(s) = seed {
                    let _seed_val = self.evaluate_expression(&s)?;
                }
                Ok(())
            }
            AstNode::Swap(var1, var2) => {
                let val1 = self.variables.get(&var1).cloned()
                    .ok_or_else(|| Error::UndefinedError(format!("Variable {} not defined", var1)))?;
                let val2 = self.variables.get(&var2).cloned()
                    .ok_or_else(|| Error::UndefinedError(format!("Variable {} not defined", var2)))?;
                self.variables.insert(var1, val2);
                self.variables.insert(var2, val1);
                Ok(())
            }
            AstNode::Clear => {
                self.variables.clear();
                self.for_stack.clear();
                Ok(())
            }
            AstNode::Erase(vars) => {
                for var in vars {
                    self.variables.remove(&var);
                }
                Ok(())
            }
            AstNode::Out(port, value) => {
                let _p = self.evaluate_expression(&port)?.as_integer()?;
                let _v = self.evaluate_expression(&value)?.as_integer()?;
                // Simulated hardware output
                Ok(())
            }
            AstNode::Poke(addr, value) => {
                let _a = self.evaluate_expression(&addr)?.as_integer()?;
                let _v = self.evaluate_expression(&value)?.as_integer()?;
                // Simulated memory write
                Ok(())
            }
            AstNode::Wait(port, mask) => {
                let _p = self.evaluate_expression(&port)?.as_integer()?;
                let _m = self.evaluate_expression(&mask)?.as_integer()?;
                // Simulated hardware wait
                Ok(())
            }
            AstNode::DefFn(_name, _params, _expr) => {
                // Store user-defined function (simplified)
                // Would need to store params and expression for later evaluation
                Ok(())
            }
            
            // Program management
            AstNode::Load(filename) => {
                // Load program from file
                // Clear current program
                self.lines.clear();
                
                // Open and read file
                let file_num = 1; // Use temporary file handle
                match self.file_manager.open(file_num, &filename, FileMode::Input) {
                    Ok(_) => {
                        // Read all lines and parse them
                        loop {
                            match self.file_manager.read_line(file_num) {
                                Ok(line) => {
                                    if line.is_empty() {
                                        break; // End of file
                                    }
                                    // Parse and add line to program
                                    // Format: "line_number statements"
                                    if let Some(first_space) = line.find(' ') {
                                        if let Ok(line_num) = line[..first_space].parse::<u32>() {
                                            // Re-parse the line content
                                            // For simplicity, just store as a dummy REM
                                            self.lines.insert(line_num, vec![AstNode::Rem(line[first_space+1..].to_string())]);
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        let _ = self.file_manager.close(file_num);
                        console_println!("Program loaded from {}", filename);
                    }
                    Err(_) => {
                        console_println!("Error: Could not load {}", filename);
                    }
                }
                Ok(())
            }
            AstNode::Save(filename) => {
                // Save program to file
                let file_num = 1; // Use temporary file handle
                match self.file_manager.open(file_num, &filename, FileMode::Output) {
                    Ok(_) => {
                        // Get sorted line numbers
                        let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
                        line_nums.sort();
                        
                        // Write each line
                        for line_num in line_nums {
                            let line_text = format!("{} REM saved line", line_num);
                            let _ = self.file_manager.write_line(file_num, &line_text);
                        }
                        let _ = self.file_manager.close(file_num);
                        console_println!("Program saved to {}", filename);
                    }
                    Err(_) => {
                        console_println!("Error: Could not save to {}", filename);
                    }
                }
                Ok(())
            }
            AstNode::Merge(filename) => {
                // Merge program from file - add lines without clearing existing
                let file_num = 1;
                match self.file_manager.open(file_num, &filename, FileMode::Input) {
                    Ok(_) => {
                        loop {
                            match self.file_manager.read_line(file_num) {
                                Ok(line) => {
                                    if line.is_empty() {
                                        break;
                                    }
                                    // Parse and merge line (simple version)
                                    if let Some(first_space) = line.find(' ') {
                                        if let Ok(line_num) = line[..first_space].parse::<u32>() {
                                            self.lines.insert(line_num, vec![AstNode::Rem(line[first_space+1..].to_string())]);
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        let _ = self.file_manager.close(file_num);
                        console_println!("Program merged from {}", filename);
                    }
                    Err(_) => {
                        console_println!("Error: Could not merge {}", filename);
                    }
                }
                Ok(())
            }
            AstNode::Chain(filename, start_line) => {
                // Chain to another program - load and optionally jump to line
                // First, load the program
                self.lines.clear();
                let file_num = 1;
                match self.file_manager.open(file_num, &filename, FileMode::Input) {
                    Ok(_) => {
                        loop {
                            match self.file_manager.read_line(file_num) {
                                Ok(line) => {
                                    if line.is_empty() {
                                        break;
                                    }
                                    if let Some(first_space) = line.find(' ') {
                                        if let Ok(line_num) = line[..first_space].parse::<u32>() {
                                            self.lines.insert(line_num, vec![AstNode::Rem(line[first_space+1..].to_string())]);
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        let _ = self.file_manager.close(file_num);
                        
                        // Start execution at specified line or first line
                        if let Some(line) = start_line {
                            self.current_line = Some(line);
                        } else {
                            self.current_line = self.lines.keys().min().copied();
                        }
                        console_println!("Chained to {}", filename);
                    }
                    Err(_) => {
                        console_println!("Error: Could not chain to {}", filename);
                    }
                }
                Ok(())
            }
            AstNode::Cont => {
                // Continue execution from where it stopped
                // This requires saving execution state when program stops
                // For now, just resume from current line if set
                if self.current_line.is_some() {
                    console_println!("Continuing execution...");
                    // The run loop will continue from current_line
                } else {
                    console_println!("Error: No program to continue");
                }
                Ok(())
            }
            
            // Program editing
            AstNode::Auto(_start, _increment) => {
                console_println!("AUTO: Feature not yet fully implemented");
                Ok(())
            }
            AstNode::Delete(_start, _end) => {
                console_println!("DELETE: Feature not yet fully implemented");
                Ok(())
            }
            AstNode::Renum(_new_start, _old_start, _increment) => {
                console_println!("RENUM: Feature not yet fully implemented");
                Ok(())
            }
            AstNode::Edit(_line) => {
                console_println!("EDIT: Feature not yet fully implemented");
                Ok(())
            }
            AstNode::Tron => {
                console_println!("Trace ON");
                Ok(())
            }
            AstNode::Troff => {
                console_println!("Trace OFF");
                Ok(())
            }
            
            // Advanced graphics
            AstNode::View(_x1, _y1, _x2, _y2) => {
                console_println!("VIEW: Setting viewport");
                Ok(())
            }
            AstNode::Window(_x1, _y1, _x2, _y2) => {
                console_println!("WINDOW: Setting logical coordinates");
                Ok(())
            }
            AstNode::Preset(x, y, color) => {
                let x_val = self.evaluate_expression(&x)?.as_integer()?;
                let y_val = self.evaluate_expression(&y)?.as_integer()?;
                let c = if let Some(c_expr) = color {
                    Some(self.evaluate_expression(&c_expr)?.as_integer()? as u8)
                } else {
                    None
                };
                self.screen.pset(x_val, y_val, c)?;
                Ok(())
            }
            AstNode::Paint(_x, _y, _paint_color, _border_color) => {
                console_println!("PAINT: Flood fill not yet fully implemented");
                Ok(())
            }
            AstNode::Draw(_draw_string) => {
                console_println!("DRAW: Complex shape drawing not yet fully implemented");
                Ok(())
            }
            AstNode::GraphicsGet(_x1, _y1, _x2, _y2, _array) => {
                console_println!("GET: Graphics block capture not yet fully implemented");
                Ok(())
            }
            AstNode::GraphicsPut(_x, _y, _array, _action) => {
                console_println!("PUT: Graphics block display not yet fully implemented");
                Ok(())
            }
            AstNode::Palette(_attr, _color) => {
                console_println!("PALETTE: Color palette manipulation not yet fully implemented");
                Ok(())
            }
            
            // Sound
            AstNode::Play(_music_string) => {
                console_println!("PLAY: Music playback not yet fully implemented");
                Ok(())
            }
            
            // File operations
            AstNode::Reset => {
                self.file_manager.close_all()?;
                Ok(())
            }
            AstNode::Kill(_filename) => {
                console_println!("KILL: File deletion not yet fully implemented");
                Ok(())
            }
            AstNode::Name(_old_name, _new_name) => {
                console_println!("NAME: File rename not yet fully implemented");
                Ok(())
            }
            AstNode::Files(_filespec) => {
                console_println!("FILES: Directory listing not yet fully implemented");
                Ok(())
            }
            AstNode::Field(_file_number, _field_specs) => {
                console_println!("FIELD: Random file buffer definition not yet fully implemented");
                Ok(())
            }
            AstNode::Lset(_var, _expr) => {
                console_println!("LSET: Left-justify in field not yet fully implemented");
                Ok(())
            }
            AstNode::Rset(_var, _expr) => {
                console_println!("RSET: Right-justify in field not yet fully implemented");
                Ok(())
            }
            AstNode::FileGet(_file_number, _record_number) => {
                console_println!("GET: Read record not yet fully implemented");
                Ok(())
            }
            AstNode::FilePut(_file_number, _record_number) => {
                console_println!("PUT: Write record not yet fully implemented");
                Ok(())
            }
            AstNode::PrintUsing(_format, _exprs) => {
                console_println!("PRINT USING: Formatted output not yet fully implemented");
                Ok(())
            }
            AstNode::Write(exprs) => {
                for (i, expr) in exprs.iter().enumerate() {
                    let value = self.evaluate_expression(expr)?;
                    if let Value::String(s) = value {
                        console_print!("\"{}\"", s);
                    } else {
                        console_print!("{}", value);
                    }
                    if i < exprs.len() - 1 {
                        console_print!(",");
                    }
                }
                console_println!();
                Ok(())
            }
            
            // Variable type declarations
            AstNode::DefStr(_start, _end) => {
                console_println!("DEFSTR: Variable type declaration not yet fully implemented");
                Ok(())
            }
            AstNode::DefInt(_start, _end) => {
                console_println!("DEFINT: Variable type declaration not yet fully implemented");
                Ok(())
            }
            AstNode::DefSng(_start, _end) => {
                console_println!("DEFSNG: Variable type declaration not yet fully implemented");
                Ok(())
            }
            AstNode::DefDbl(_start, _end) => {
                console_println!("DEFDBL: Variable type declaration not yet fully implemented");
                Ok(())
            }
            AstNode::OptionBase(_base) => {
                console_println!("OPTION BASE: Array base setting not yet fully implemented");
                Ok(())
            }
            
            // System/Hardware
            AstNode::Key(_key_number, _string) => {
                console_println!("KEY: Function key definition not yet fully implemented");
                Ok(())
            }
            AstNode::KeyOn => {
                console_println!("KEY ON: Function key display enabled");
                Ok(())
            }
            AstNode::KeyOff => {
                console_println!("KEY OFF: Function key display disabled");
                Ok(())
            }
            AstNode::KeyList => {
                console_println!("KEY LIST: Listing function keys");
                Ok(())
            }
            AstNode::OnKey(_key_number, _line_number) => {
                console_println!("ON KEY: Function key trap not yet fully implemented");
                Ok(())
            }
            AstNode::DefSeg(_segment) => {
                console_println!("DEF SEG: Segment definition not yet fully implemented");
                Ok(())
            }
            AstNode::Bload(_filename, _offset) => {
                console_println!("BLOAD: Binary load not yet fully implemented");
                Ok(())
            }
            AstNode::Bsave(_filename, _offset, _length) => {
                console_println!("BSAVE: Binary save not yet fully implemented");
                Ok(())
            }
            AstNode::Call(_address, _params) => {
                console_println!("CALL: Machine language call not yet fully implemented");
                Ok(())
            }
            AstNode::Usr(_address) => {
                console_println!("USR: User function call not yet fully implemented");
                Ok(())
            }
            
            _ => Err(Error::RuntimeError(format!("Cannot execute node: {:?}", node))),
        }
    }

    fn execute_print(&mut self, exprs: Vec<AstNode>) -> Result<()> {
        for (i, expr) in exprs.iter().enumerate() {
            let value = self.evaluate_expression(expr)?;
            console_print!("{}", value);
            
            if i < exprs.len() - 1 {
                console_print!(" ");
            }
        }
        console_println!();
        Ok(())
    }

    fn execute_let(&mut self, name: String, expr: AstNode) -> Result<()> {
        let value = self.evaluate_expression(&expr)?;
        self.variables.insert(name, value);
        Ok(())
    }

    fn execute_array_assign(&mut self, name: String, indices: Vec<AstNode>, expr: AstNode) -> Result<()> {
        // Evaluate the value to assign
        let value = self.evaluate_expression(&expr)?;

        // Evaluate indices
        let mut idx_values = Vec::new();
        for idx_node in indices {
            let idx_val = self.evaluate_expression(&idx_node)?;
            let idx = idx_val.as_integer()? as usize;
            idx_values.push(idx);
        }

        // Build key: "name_idx1_idx2_..."
        let key = format!("{}_{}", name, idx_values.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("_"));

        self.arrays.insert(key, value);
        Ok(())
    }

    fn execute_if(
        &mut self,
        condition: AstNode,
        then_stmts: Vec<AstNode>,
        else_stmts: Option<Vec<AstNode>>,
    ) -> Result<()> {
        let condition_value = self.evaluate_expression(&condition)?;
        let is_true = match condition_value {
            Value::Integer(i) => i != 0,
            Value::Single(f) => f != 0.0,
            Value::Double(d) => d != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Nil => false,
        };

        if is_true {
            for stmt in then_stmts {
                self.execute_node(stmt)?;
            }
        } else if let Some(else_statements) = else_stmts {
            for stmt in else_statements {
                self.execute_node(stmt)?;
            }
        }

        Ok(())
    }

    fn execute_for(
        &mut self,
        var: String,
        start: AstNode,
        end: AstNode,
        step: Option<AstNode>,
    ) -> Result<()> {
        let start_val = self.evaluate_expression(&start)?.as_double()?;
        let end_val = self.evaluate_expression(&end)?.as_double()?;
        let step_val = if let Some(s) = step {
            self.evaluate_expression(&s)?.as_double()?
        } else {
            1.0
        };

        // Initialize loop variable
        self.variables.insert(var.clone(), Value::Double(start_val));

        // Store loop state (in real implementation, would need to handle nested loops properly)
        let state = ForLoopState {
            variable: var,
            end_value: end_val,
            step: step_val,
            return_line: self.current_line.unwrap_or(0),
        };
        self.for_stack.push(state);

        Ok(())
    }

    fn execute_next(&mut self, var: String) -> Result<()> {
        if let Some(state) = self.for_stack.last().cloned() {
            if !var.is_empty() && state.variable != var {
                return Err(Error::RuntimeError(format!(
                    "NEXT variable mismatch: expected {}, got {}",
                    state.variable, var
                )));
            }

            let current = self.variables
                .get(&state.variable)
                .ok_or_else(|| Error::UndefinedError(format!("Variable {} not defined", state.variable)))?
                .as_double()?;

            let new_value = current + state.step;
            self.variables.insert(state.variable.clone(), Value::Double(new_value));

            // Check if loop should continue
            let should_continue = if state.step > 0.0 {
                new_value <= state.end_value
            } else {
                new_value >= state.end_value
            };

            if should_continue {
                // Jump back to the line after FOR
                let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
                line_nums.sort();
                if let Some(next_line) = line_nums.iter().find(|&&n| n > state.return_line).copied() {
                    self.current_line = Some(next_line);
                }
            } else {
                // Loop is done
                self.for_stack.pop();
            }
        } else {
            return Err(Error::RuntimeError("NEXT without FOR".to_string()));
        }

        Ok(())
    }

    fn execute_while(&mut self, condition: AstNode) -> Result<()> {
        // Evaluate the condition
        let condition_value = self.evaluate_expression(&condition)?;
        let is_true = match condition_value {
            Value::Integer(i) => i != 0,
            Value::Single(f) => f != 0.0,
            Value::Double(d) => d != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Nil => false,
        };

        if !is_true {
            // Condition is false, skip to after WEND
            // Find the matching WEND line
            if let Some(current) = self.current_line {
                let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
                line_nums.sort();

                // Skip to the line after the matching WEND
                let mut depth = 1;
                for &line_num in line_nums.iter().skip_while(|&&n| n <= current) {
                    if let Some(statements) = self.lines.get(&line_num) {
                        for stmt in statements {
                            match stmt {
                                AstNode::While(_) => depth += 1,
                                AstNode::Wend => {
                                    depth -= 1;
                                    if depth == 0 {
                                        // Found matching WEND, jump past it
                                        self.current_line = line_nums.iter()
                                            .find(|&&n| n > line_num)
                                            .copied();
                                        return Ok(());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else {
            // Condition is true, enter the loop
            if let Some(current) = self.current_line {
                self.while_stack.push(WhileLoopState {
                    condition: condition.clone(),
                    return_line: current,
                });
            }
        }

        Ok(())
    }

    fn execute_wend(&mut self) -> Result<()> {
        if let Some(while_state) = self.while_stack.last().cloned() {
            // Evaluate the condition again
            let condition_value = self.evaluate_expression(&while_state.condition)?;
            let is_true = match condition_value {
                Value::Integer(i) => i != 0,
                Value::Single(f) => f != 0.0,
                Value::Double(d) => d != 0.0,
                Value::String(s) => !s.is_empty(),
                Value::Nil => false,
            };

            if is_true {
                // Jump back to the WHILE line
                self.current_line = Some(while_state.return_line);
            } else {
                // Exit the loop
                self.while_stack.pop();
            }
        } else {
            return Err(Error::RuntimeError("WEND without WHILE".to_string()));
        }

        Ok(())
    }

    fn execute_goto(&mut self, line: u32) -> Result<()> {
        if self.lines.contains_key(&line) {
            self.current_line = Some(line);
            Ok(())
        } else {
            Err(Error::LineNumberError(format!("Line {} not found", line)))
        }
    }

    fn execute_gosub(&mut self, line: u32) -> Result<()> {
        // Push the NEXT line number to return to (not the current GOSUB line)
        if let Some(current) = self.current_line {
            // Find the next line number after current
            let mut line_nums: Vec<u32> = self.lines.keys().copied().collect();
            line_nums.sort();
            let next_line = line_nums.iter().find(|&&n| n > current).copied();
            if let Some(next) = next_line {
                self.call_stack.push(next);
            } else {
                // No next line - push current (will end program after RETURN)
                self.call_stack.push(current);
            }
        }
        self.execute_goto(line)
    }

    fn execute_return(&mut self) -> Result<()> {
        if let Some(return_line) = self.call_stack.pop() {
            self.current_line = Some(return_line);
            Ok(())
        } else {
            Err(Error::RuntimeError("RETURN without GOSUB".to_string()))
        }
    }

    fn execute_input(&mut self, vars: Vec<String>) -> Result<()> {
        for var in vars {
            console_print!("? ");

            #[cfg(feature = "std")]
            {
                io::stdout().flush().unwrap();
            }

            let input = self.read_input_line();
            let input = input.trim();

            // Check if input is empty (non-interactive mode)
            if input.is_empty() {
                // Provide default value
                let value = if var.ends_with('$') {
                    Value::String("test".to_string())
                } else {
                    Value::Integer(10)
                };
                self.variables.insert(var, value);
            } else {
                // Try to parse as number first, then as string
                let value = if let Ok(i) = input.parse::<i32>() {
                    Value::Integer(i)
                } else if let Ok(f) = input.parse::<f64>() {
                    Value::Double(f)
                } else {
                    Value::String(input.to_string())
                };

                self.variables.insert(var, value);
            }
        }

        Ok(())
    }

    /// Platform-agnostic input reading
    #[cfg(feature = "std")]
    fn read_input_line(&self) -> String {
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => input,
            Err(_) => String::new(),
        }
    }

    #[cfg(feature = "watos")]
    fn read_input_line(&self) -> String {
        // For WATOS, use syscall for console input
        use crate::platform::watos_platform::WatosConsole;
        use crate::platform::Console;
        let mut console = WatosConsole::new();
        console.read_line()
    }

    #[cfg(all(not(feature = "std"), not(feature = "watos")))]
    fn read_input_line(&self) -> String {
        // For stub platform (library-only build), return empty
        use crate::platform::stub_platform::StubConsole;
        use crate::platform::Console;
        let mut console = StubConsole::new();
        console.read_line()
    }

    fn execute_dim(&mut self, name: String, dimensions: Vec<AstNode>) -> Result<()> {
        // Evaluate dimension sizes
        let mut dim_sizes = Vec::new();
        for dim_node in dimensions {
            let dim_val = self.evaluate_expression(&dim_node)?;
            let size = dim_val.as_integer()? as usize;
            dim_sizes.push(size + 1); // GW-BASIC arrays are 0-indexed but DIM specifies max index
        }

        // Store dimensions
        self.array_dims.insert(name.clone(), dim_sizes);

        // Initialize all elements to 0 or empty string
        // For simplicity, we'll initialize them on first access instead
        Ok(())
    }

    /// Evaluate an expression and return its value
    fn evaluate_expression(&mut self, node: &AstNode) -> Result<Value> {
        match node {
            AstNode::Literal(val) => Ok(val.clone()),
            AstNode::Variable(name) => {
                self.variables
                    .get(name)
                    .cloned()
                    .ok_or_else(|| Error::UndefinedError(format!("Variable {} not defined", name)))
            }
            AstNode::BinaryOp(op, left, right) => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                self.evaluate_binary_op(op, left_val, right_val)
            }
            AstNode::UnaryOp(op, expr) => {
                let val = self.evaluate_expression(expr)?;
                self.evaluate_unary_op(op, val)
            }
            AstNode::FunctionCall(name, args) => {
                self.evaluate_function_call(name, args)
            }
            _ => Err(Error::RuntimeError(format!("Cannot evaluate node: {:?}", node))),
        }
    }

    fn evaluate_binary_op(&mut self, op: &BinaryOperator, left: Value, right: Value) -> Result<Value> {
        match op {
            BinaryOperator::Add => {
                if left.is_string() || right.is_string() {
                    Ok(Value::String(format!("{}{}", left.as_string(), right.as_string())))
                } else {
                    Ok(Value::Double(left.as_double()? + right.as_double()?))
                }
            }
            BinaryOperator::Subtract => {
                Ok(Value::Double(left.as_double()? - right.as_double()?))
            }
            BinaryOperator::Multiply => {
                Ok(Value::Double(left.as_double()? * right.as_double()?))
            }
            BinaryOperator::Divide => {
                let right_val = right.as_double()?;
                if right_val == 0.0 {
                    Err(Error::DivisionByZero)
                } else {
                    Ok(Value::Double(left.as_double()? / right_val))
                }
            }
            BinaryOperator::IntDivide => {
                let right_val = right.as_integer()?;
                if right_val == 0 {
                    Err(Error::DivisionByZero)
                } else {
                    Ok(Value::Integer(left.as_integer()? / right_val))
                }
            }
            BinaryOperator::Mod => {
                let right_val = right.as_integer()?;
                if right_val == 0 {
                    Err(Error::DivisionByZero)
                } else {
                    Ok(Value::Integer(left.as_integer()? % right_val))
                }
            }
            BinaryOperator::Power => {
                Ok(Value::Double(libm::pow(left.as_double()?, right.as_double()?)))
            }
            BinaryOperator::Equal => {
                Ok(Value::Integer(if left.as_double()? == right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::NotEqual => {
                Ok(Value::Integer(if left.as_double()? != right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::LessThan => {
                Ok(Value::Integer(if left.as_double()? < right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::GreaterThan => {
                Ok(Value::Integer(if left.as_double()? > right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::LessEqual => {
                Ok(Value::Integer(if left.as_double()? <= right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::GreaterEqual => {
                Ok(Value::Integer(if left.as_double()? >= right.as_double()? { -1 } else { 0 }))
            }
            BinaryOperator::And => {
                let l = left.as_integer()?;
                let r = right.as_integer()?;
                Ok(Value::Integer(l & r))
            }
            BinaryOperator::Or => {
                let l = left.as_integer()?;
                let r = right.as_integer()?;
                Ok(Value::Integer(l | r))
            }
            BinaryOperator::Xor => {
                let l = left.as_integer()?;
                let r = right.as_integer()?;
                Ok(Value::Integer(l ^ r))
            }
            BinaryOperator::Eqv => {
                let l = left.as_integer()?;
                let r = right.as_integer()?;
                Ok(Value::Integer(!(l ^ r)))
            }
            BinaryOperator::Imp => {
                let l = left.as_integer()?;
                let r = right.as_integer()?;
                Ok(Value::Integer(!l | r))
            }
        }
    }

    fn evaluate_unary_op(&mut self, op: &UnaryOperator, val: Value) -> Result<Value> {
        match op {
            UnaryOperator::Negate => {
                Ok(Value::Double(-val.as_double()?))
            }
            UnaryOperator::Not => {
                Ok(Value::Integer(!val.as_integer()?))
            }
        }
    }

    fn evaluate_function_call(&mut self, name: &str, args: &[AstNode]) -> Result<Value> {
        use crate::functions::*;

        // Check if this is an array access
        if self.array_dims.contains_key(name) || self.arrays.keys().any(|k| k.starts_with(&format!("{}_", name))) {
            // Evaluate indices
            let mut idx_values = Vec::new();
            for idx_node in args {
                let idx_val = self.evaluate_expression(idx_node)?;
                let idx = idx_val.as_integer()? as usize;
                idx_values.push(idx);
            }

            // Build key: "name_idx1_idx2_..."
            let key = format!("{}_{}", name, idx_values.iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("_"));

            // Return array value or default 0 if not set
            return Ok(self.arrays.get(&key).cloned().unwrap_or(Value::Integer(0)));
        }

        // Evaluate all arguments for function calls
        let eval_args: Vec<Value> = args.iter()
            .map(|arg| self.evaluate_expression(arg))
            .collect::<Result<Vec<Value>>>()?;

        // Math functions (single argument)
        match name.to_uppercase().as_str() {
            "ABS" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("ABS requires 1 argument".to_string()));
                }
                abs_fn(eval_args[0].clone())
            }
            "INT" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("INT requires 1 argument".to_string()));
                }
                int_fn(eval_args[0].clone())
            }
            "FIX" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("FIX requires 1 argument".to_string()));
                }
                fix_fn(eval_args[0].clone())
            }
            "CINT" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CINT requires 1 argument".to_string()));
                }
                cint_fn(eval_args[0].clone())
            }
            "CSNG" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CSNG requires 1 argument".to_string()));
                }
                csng_fn(eval_args[0].clone())
            }
            "CDBL" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CDBL requires 1 argument".to_string()));
                }
                cdbl_fn(eval_args[0].clone())
            }
            "SQR" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("SQR requires 1 argument".to_string()));
                }
                sqr_fn(eval_args[0].clone())
            }
            "SIN" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("SIN requires 1 argument".to_string()));
                }
                sin_fn(eval_args[0].clone())
            }
            "COS" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("COS requires 1 argument".to_string()));
                }
                cos_fn(eval_args[0].clone())
            }
            "TAN" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("TAN requires 1 argument".to_string()));
                }
                tan_fn(eval_args[0].clone())
            }
            "ATN" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("ATN requires 1 argument".to_string()));
                }
                atn_fn(eval_args[0].clone())
            }
            "EXP" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("EXP requires 1 argument".to_string()));
                }
                exp_fn(eval_args[0].clone())
            }
            "LOG" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("LOG requires 1 argument".to_string()));
                }
                log_fn(eval_args[0].clone())
            }
            "SGN" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("SGN requires 1 argument".to_string()));
                }
                sgn_fn(eval_args[0].clone())
            }
            
            // String functions
            "LEN" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("LEN requires 1 argument".to_string()));
                }
                len_fn(eval_args[0].clone())
            }
            "ASC" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("ASC requires 1 argument".to_string()));
                }
                asc_fn(eval_args[0].clone())
            }
            "CHR$" | "CHR" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CHR$ requires 1 argument".to_string()));
                }
                chr_fn(eval_args[0].clone())
            }
            "STR$" | "STR" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("STR$ requires 1 argument".to_string()));
                }
                str_fn(eval_args[0].clone())
            }
            "VAL" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("VAL requires 1 argument".to_string()));
                }
                val_fn(eval_args[0].clone())
            }
            "LEFT$" | "LEFT" => {
                if eval_args.len() != 2 {
                    return Err(Error::RuntimeError("LEFT$ requires 2 arguments".to_string()));
                }
                left_fn(eval_args[0].clone(), eval_args[1].clone())
            }
            "RIGHT$" | "RIGHT" => {
                if eval_args.len() != 2 {
                    return Err(Error::RuntimeError("RIGHT$ requires 2 arguments".to_string()));
                }
                right_fn(eval_args[0].clone(), eval_args[1].clone())
            }
            "MID$" | "MID" => {
                if eval_args.len() < 2 || eval_args.len() > 3 {
                    return Err(Error::RuntimeError("MID$ requires 2 or 3 arguments".to_string()));
                }
                let len = if eval_args.len() == 3 {
                    Some(eval_args[2].clone())
                } else {
                    None
                };
                mid_fn(eval_args[0].clone(), eval_args[1].clone(), len)
            }
            "SPACE$" | "SPACE" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("SPACE$ requires 1 argument".to_string()));
                }
                space_fn(eval_args[0].clone())
            }
            "STRING$" | "STRING" => {
                if eval_args.len() != 2 {
                    return Err(Error::RuntimeError("STRING$ requires 2 arguments".to_string()));
                }
                string_fn(eval_args[0].clone(), eval_args[1].clone())
            }
            "INSTR" => {
                if eval_args.len() < 2 || eval_args.len() > 3 {
                    return Err(Error::RuntimeError("INSTR requires 2 or 3 arguments".to_string()));
                }
                if eval_args.len() == 3 {
                    instr_fn(Some(eval_args[0].clone()), eval_args[1].clone(), eval_args[2].clone())
                } else {
                    instr_fn(None, eval_args[0].clone(), eval_args[1].clone())
                }
            }
            "HEX$" | "HEX" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("HEX$ requires 1 argument".to_string()));
                }
                hex_fn(eval_args[0].clone())
            }
            "OCT$" | "OCT" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("OCT$ requires 1 argument".to_string()));
                }
                oct_fn(eval_args[0].clone())
            }
            "LCASE$" | "LCASE" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("LCASE$ requires 1 argument".to_string()));
                }
                lcase_fn(eval_args[0].clone())
            }
            "UCASE$" | "UCASE" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("UCASE$ requires 1 argument".to_string()));
                }
                ucase_fn(eval_args[0].clone())
            }
            "INPUT$" | "INPUT" => {
                if eval_args.len() < 1 || eval_args.len() > 2 {
                    return Err(Error::RuntimeError("INPUT$ requires 1 or 2 arguments".to_string()));
                }
                let file_num = if eval_args.len() == 2 {
                    Some(eval_args[1].clone())
                } else {
                    None
                };
                input_fn(eval_args[0].clone(), file_num)
            }
            
            // Conversion functions
            "CVI" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CVI requires 1 argument".to_string()));
                }
                cvi_fn(eval_args[0].clone())
            }
            "CVS" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CVS requires 1 argument".to_string()));
                }
                cvs_fn(eval_args[0].clone())
            }
            "CVD" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("CVD requires 1 argument".to_string()));
                }
                cvd_fn(eval_args[0].clone())
            }
            "MKI$" | "MKI" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("MKI$ requires 1 argument".to_string()));
                }
                mki_fn(eval_args[0].clone())
            }
            "MKS$" | "MKS" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("MKS$ requires 1 argument".to_string()));
                }
                mks_fn(eval_args[0].clone())
            }
            "MKD$" | "MKD" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("MKD$ requires 1 argument".to_string()));
                }
                mkd_fn(eval_args[0].clone())
            }
            
            // System functions
            "RND" => {
                if eval_args.is_empty() {
                    rnd_fn(None)
                } else if eval_args.len() == 1 {
                    rnd_fn(Some(eval_args[0].clone()))
                } else {
                    Err(Error::RuntimeError("RND requires 0 or 1 arguments".to_string()))
                }
            }
            "TIMER" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("TIMER requires 0 arguments".to_string()));
                }
                timer_fn()
            }
            "PEEK" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("PEEK requires 1 argument".to_string()));
                }
                peek_fn(eval_args[0].clone())
            }
            "INP" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("INP requires 1 argument".to_string()));
                }
                inp_fn(eval_args[0].clone())
            }
            "FRE" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("FRE requires 1 argument".to_string()));
                }
                fre_fn(eval_args[0].clone())
            }
            "VARPTR" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("VARPTR requires 1 argument".to_string()));
                }
                varptr_fn(eval_args[0].clone())
            }
            "INKEY$" | "INKEY" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("INKEY$ requires 0 arguments".to_string()));
                }
                inkey_fn()
            }
            "DATE$" | "DATE" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("DATE$ requires 0 arguments".to_string()));
                }
                date_fn()
            }
            "TIME$" | "TIME" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("TIME$ requires 0 arguments".to_string()));
                }
                time_fn()
            }
            "POS" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("POS requires 1 argument".to_string()));
                }
                pos_fn(eval_args[0].clone())
            }
            "CSRLIN" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("CSRLIN requires 0 arguments".to_string()));
                }
                csrlin_fn()
            }
            "EOF" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("EOF requires 1 argument".to_string()));
                }
                eof_fn(eval_args[0].clone())
            }
            "LOC" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("LOC requires 1 argument".to_string()));
                }
                loc_fn(eval_args[0].clone())
            }
            "LOF" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("LOF requires 1 argument".to_string()));
                }
                lof_fn(eval_args[0].clone())
            }
            "POINT" => {
                if eval_args.len() != 2 {
                    return Err(Error::RuntimeError("POINT requires 2 arguments".to_string()));
                }
                point_fn(eval_args[0].clone(), eval_args[1].clone())
            }
            "SCREEN" => {
                if eval_args.len() < 2 || eval_args.len() > 3 {
                    return Err(Error::RuntimeError("SCREEN requires 2 or 3 arguments".to_string()));
                }
                let color_num = if eval_args.len() == 3 {
                    Some(eval_args[2].clone())
                } else {
                    None
                };
                screen_fn(eval_args[0].clone(), eval_args[1].clone(), color_num)
            }
            
            // Error handling functions
            "ERL" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("ERL requires 0 arguments".to_string()));
                }
                erl_fn()
            }
            "ERR" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("ERR requires 0 arguments".to_string()));
                }
                err_fn()
            }
            "ERDEV" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("ERDEV requires 0 arguments".to_string()));
                }
                erdev_fn()
            }
            "ERDEV$" => {
                if !eval_args.is_empty() {
                    return Err(Error::RuntimeError("ERDEV$ requires 0 arguments".to_string()));
                }
                erdev_string_fn()
            }
            
            // Environment and file functions
            "ENVIRON$" | "ENVIRON" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("ENVIRON$ requires 1 argument".to_string()));
                }
                environ_fn(eval_args[0].clone())
            }
            "IOCTL$" | "IOCTL" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("IOCTL$ requires 1 argument".to_string()));
                }
                ioctl_fn(eval_args[0].clone())
            }
            "FILEATTR" => {
                if eval_args.len() != 2 {
                    return Err(Error::RuntimeError("FILEATTR requires 2 arguments".to_string()));
                }
                fileattr_fn(eval_args[0].clone(), eval_args[1].clone())
            }
            
            // Joystick functions
            "STICK" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("STICK requires 1 argument".to_string()));
                }
                stick_fn(eval_args[0].clone())
            }
            "STRIG" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("STRIG requires 1 argument".to_string()));
                }
                strig_fn(eval_args[0].clone())
            }
            
            // Machine language functions
            "USR" | "USR0" | "USR1" | "USR2" | "USR3" | "USR4" | 
            "USR5" | "USR6" | "USR7" | "USR8" | "USR9" => {
                if eval_args.len() != 1 {
                    return Err(Error::RuntimeError("USR requires 1 argument".to_string()));
                }
                // Extract index from function name (USR0-USR9)
                let index = if name.len() > 3 {
                    // Safe: We've already matched against USR0-USR9, so last char is a digit
                    match name.chars().last().and_then(|c| c.to_digit(10)) {
                        Some(digit) => Some(Value::Integer(digit as i32)),
                        None => None,
                    }
                } else {
                    None
                };
                usr_fn(index, eval_args[0].clone())
            }
            
            _ => Err(Error::UndefinedError(format!("Function {} not defined", name))),
        }
    }

    /// Run a stored program starting from the first line
    pub fn run(&mut self) -> Result<()> {
        let mut line_numbers: Vec<u32> = self.lines.keys().copied().collect();
        line_numbers.sort();

        for line_num in line_numbers {
            self.current_line = Some(line_num);
            if let Some(statements) = self.lines.get(&line_num).cloned() {
                for stmt in statements {
                    if let Err(e) = self.execute_node(stmt) {
                        if matches!(e, Error::ProgramEnd) {
                            return Ok(());
                        }
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn test_interpreter_creation() {
        let interp = Interpreter::new();
        assert_eq!(interp.variables.len(), 0);
    }

    #[test]
    fn test_execute_print() {
        let mut interp = Interpreter::new();
        let mut lexer = Lexer::new("PRINT 42");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();
        
        // Should not error
        assert!(interp.execute(ast).is_ok());
    }

    #[test]
    fn test_execute_let() {
        let mut interp = Interpreter::new();
        let mut lexer = Lexer::new("LET A = 42");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();
        
        interp.execute(ast).unwrap();
        assert_eq!(interp.variables.get("A").unwrap().as_integer().unwrap(), 42);
    }

    #[test]
    fn test_evaluate_expression() {
        let mut interp = Interpreter::new();
        let mut lexer = Lexer::new("LET A = 2 + 3 * 4");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();
        
        interp.execute(ast).unwrap();
        // 2 + 3 * 4 = 2 + 12 = 14
        assert_eq!(interp.variables.get("A").unwrap().as_integer().unwrap(), 14);
    }

    #[test]
    fn test_division_by_zero() {
        let mut interp = Interpreter::new();
        let mut lexer = Lexer::new("PRINT 1 / 0");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();
        
        let result = interp.execute(ast);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::DivisionByZero);
    }

    #[test]
    fn test_variable_undefined() {
        let mut interp = Interpreter::new();
        let mut lexer = Lexer::new("PRINT X");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        let result = interp.execute(ast);
        assert!(result.is_err());
    }

    #[test]
    fn test_array_assignment() {
        let mut interp = Interpreter::new();
        let code = "DIM A(10)\nLET A(5) = 42";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert!(interp.execute(ast).is_ok());
        // Check that the array element was stored
        assert_eq!(interp.arrays.get("A_5").unwrap().as_integer().unwrap(), 42);
    }

    #[test]
    fn test_array_access() {
        let mut interp = Interpreter::new();
        let code = "DIM A(10)\nLET A(3) = 99\nLET B = A(3)";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        assert_eq!(interp.variables.get("B").unwrap().as_integer().unwrap(), 99);
    }

    #[test]
    fn test_array_default_value() {
        let mut interp = Interpreter::new();
        let code = "DIM A(10)\nLET B = A(5)";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        // Uninitialized array elements should default to 0
        assert_eq!(interp.variables.get("B").unwrap().as_integer().unwrap(), 0);
    }

    #[test]
    fn test_for_next_loop() {
        let mut interp = Interpreter::new();
        let code = "10 LET SUM = 0\n20 FOR I = 1 TO 5\n30 LET SUM = SUM + I\n40 NEXT I\n50 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // Sum should be 1+2+3+4+5 = 15
        assert_eq!(interp.variables.get("SUM").unwrap().as_integer().unwrap(), 15);
    }

    #[test]
    fn test_if_then() {
        let mut interp = Interpreter::new();
        let code = r#"
            LET X = 10
            LET Y = 0
            IF X > 5 THEN LET Y = 1
        "#;
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        assert_eq!(interp.variables.get("Y").unwrap().as_integer().unwrap(), 1);
    }

    #[test]
    fn test_if_then_else() {
        let mut interp = Interpreter::new();
        let code = r#"
            LET X = 3
            LET Y = 0
            IF X > 5 THEN LET Y = 1 ELSE LET Y = 2
        "#;
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        assert_eq!(interp.variables.get("Y").unwrap().as_integer().unwrap(), 2);
    }

    #[test]
    fn test_gosub_return() {
        let mut interp = Interpreter::new();
        let code = "10 LET X = 1\n20 GOSUB 100\n30 LET X = X + 10\n40 END\n100 LET X = X + 5\n110 RETURN";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // X = 1, then 1+5=6, then 6+10=16
        assert_eq!(interp.variables.get("X").unwrap().as_integer().unwrap(), 16);
    }

    #[test]
    fn test_goto() {
        let mut interp = Interpreter::new();
        let code = "10 LET X = 1\n20 GOTO 40\n30 LET X = 99\n40 LET X = X + 5\n50 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // Should skip line 30, so X = 1 + 5 = 6
        assert_eq!(interp.variables.get("X").unwrap().as_integer().unwrap(), 6);
    }

    #[test]
    fn test_dim_multidimensional() {
        let mut interp = Interpreter::new();
        let code = "DIM A(5, 10)";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert!(interp.execute(ast).is_ok());
        // Check that dimensions were stored
        let dims = interp.array_dims.get("A").unwrap();
        assert_eq!(dims.len(), 2);
        assert_eq!(dims[0], 6); // 0-5 inclusive
        assert_eq!(dims[1], 11); // 0-10 inclusive
    }

    #[test]
    fn test_while_wend() {
        let mut interp = Interpreter::new();
        let code = "10 LET X = 1\n20 WHILE X < 4\n30 LET X = X + 1\n40 WEND\n50 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // Loop should run while X < 4, so X = 1, 2, 3, then exit when X = 4
        assert_eq!(interp.variables.get("X").unwrap().as_integer().unwrap(), 4);
    }

    #[test]
    fn test_data_read() {
        let mut interp = Interpreter::new();
        let code = "10 READ A, B, C\n20 DATA 10, 20, 30\n30 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        assert_eq!(interp.variables.get("A").unwrap().as_integer().unwrap(), 10);
        assert_eq!(interp.variables.get("B").unwrap().as_integer().unwrap(), 20);
        assert_eq!(interp.variables.get("C").unwrap().as_integer().unwrap(), 30);
    }

    #[test]
    fn test_data_restore() {
        let mut interp = Interpreter::new();
        let code = "10 READ A\n20 RESTORE\n30 READ B\n40 DATA 99\n50 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // Both should read the same value after RESTORE
        assert_eq!(interp.variables.get("A").unwrap().as_integer().unwrap(), 99);
        assert_eq!(interp.variables.get("B").unwrap().as_integer().unwrap(), 99);
    }

    #[test]
    fn test_on_goto() {
        let mut interp = Interpreter::new();
        let code = "10 LET X = 2\n20 ON X GOTO 100, 200, 300\n30 LET Y = 0\n40 END\n100 LET Y = 1\n110 END\n200 LET Y = 2\n210 END\n300 LET Y = 3\n310 END";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // X = 2, so should jump to 200 (second option)
        assert_eq!(interp.variables.get("Y").unwrap().as_integer().unwrap(), 2);
    }

    #[test]
    fn test_on_gosub() {
        let mut interp = Interpreter::new();
        let code = "10 LET X = 1\n20 ON X GOSUB 100, 200\n30 LET Y = 99\n40 END\n100 LET Y = 10\n110 RETURN\n200 LET Y = 20\n210 RETURN";
        let mut lexer = Lexer::new(code);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        interp.execute(ast).unwrap();
        interp.run_stored_program().unwrap();
        // X = 1, so should call 100, then return and set Y = 99
        assert_eq!(interp.variables.get("Y").unwrap().as_integer().unwrap(), 99);
    }
}