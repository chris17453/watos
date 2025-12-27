//! Parser for GW-BASIC

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, string::{String, ToString}, vec::Vec, vec, format};

use crate::error::{Error, Result};
use crate::lexer::{Token, TokenType};
use crate::value::Value;

/// AST node types
#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    // Statements - Basic I/O
    Print(Vec<AstNode>),
    Input(Vec<String>),
    Let(String, Box<AstNode>),
    ArrayAssign(String, Vec<AstNode>, Box<AstNode>),  // name, indices, value

    // Statements - Control Flow
    If(Box<AstNode>, Vec<AstNode>, Option<Vec<AstNode>>),
    For(String, Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>),
    Next(String),
    While(Box<AstNode>),
    Wend,
    Goto(u32),
    Gosub(u32),
    OnGoto(Box<AstNode>, Vec<u32>),
    OnGosub(Box<AstNode>, Vec<u32>),
    Return,
    End,
    Stop,
    
    // Statements - Data
    Dim(String, Vec<AstNode>),
    Read(Vec<String>),
    Data(Vec<AstNode>),
    Restore(Option<u32>),
    Rem(String),
    
    // Statements - Screen/Graphics
    Cls,
    Locate(Box<AstNode>, Box<AstNode>),
    Color(Option<Box<AstNode>>, Option<Box<AstNode>>),
    Screen(Box<AstNode>),
    Width(Box<AstNode>),
    View(Option<Box<AstNode>>, Option<Box<AstNode>>, Option<Box<AstNode>>, Option<Box<AstNode>>), // x1, y1, x2, y2
    Window(Option<Box<AstNode>>, Option<Box<AstNode>>, Option<Box<AstNode>>, Option<Box<AstNode>>), // x1, y1, x2, y2
    Pset(Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>),
    Preset(Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>),
    DrawLine(Box<AstNode>, Box<AstNode>, Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>),
    Circle(Box<AstNode>, Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>),
    Paint(Box<AstNode>, Box<AstNode>, Option<Box<AstNode>>, Option<Box<AstNode>>), // x, y, paint_color, border_color
    Draw(String),                            // draw string
    GraphicsGet(Box<AstNode>, Box<AstNode>, Box<AstNode>, Box<AstNode>, String), // x1, y1, x2, y2, array
    GraphicsPut(Box<AstNode>, Box<AstNode>, String, Option<String>), // x, y, array, action
    Palette(Box<AstNode>, Box<AstNode>),    // attribute, color
    
    // Statements - Sound
    Beep,
    Sound(Box<AstNode>, Box<AstNode>),
    Play(String),                            // music string
    
    // Statements - File I/O
    Open(String, Box<AstNode>, String),  // filename, file_number, mode
    Close(Vec<i32>),
    Reset,                                   // close all files
    PrintFile(Box<AstNode>, Vec<AstNode>),  // file_number, expressions
    InputFile(Box<AstNode>, Vec<String>),   // file_number, variables
    WriteFile(Box<AstNode>, Vec<AstNode>),  // file_number, expressions
    LineInput(Vec<String>),                  // variables
    LineInputFile(Box<AstNode>, String),    // file_number, variable
    Kill(String),                            // filename
    Name(String, String),                    // old_name, new_name
    Files(Option<String>),                   // optional filespec
    Field(Box<AstNode>, Vec<(u32, String)>), // file_number, field_specs (width, var)
    Lset(String, Box<AstNode>),             // variable, expression
    Rset(String, Box<AstNode>),             // variable, expression
    FileGet(Box<AstNode>, Option<Box<AstNode>>), // file_number, optional record_number
    FilePut(Box<AstNode>, Option<Box<AstNode>>), // file_number, optional record_number
    PrintUsing(String, Vec<AstNode>),        // format string, expressions
    Write(Vec<AstNode>),                     // expressions to screen
    
    // Statements - Program Control
    List(Option<u32>, Option<u32>),         // start_line, end_line
    New,
    Run(Option<u32>),                       // optional start line
    Load(String),                           // filename
    Save(String),                           // filename
    Merge(String),                          // filename
    Chain(String, Option<u32>),             // filename, optional line
    Cont,                                   // continue after STOP
    
    // Statements - Program Editing
    Auto(Option<u32>, Option<u32>),         // start, increment
    Delete(u32, Option<u32>),               // start, optional end
    Renum(Option<u32>, Option<u32>, Option<u32>), // new_start, old_start, increment
    Edit(u32),                              // line number
    Tron,                                   // trace on
    Troff,                                  // trace off
    
    // Statements - Error Handling
    OnError(u32),                           // line number for error handler
    Resume(Option<u32>),                    // optional line number
    ErrorStmt(Box<AstNode>),                // error number to raise
    
    // Statements - System
    Randomize(Option<Box<AstNode>>),
    Swap(String, String),
    Clear,
    Erase(Vec<String>),
    Out(Box<AstNode>, Box<AstNode>),       // port, value
    Poke(Box<AstNode>, Box<AstNode>),      // address, value
    Wait(Box<AstNode>, Box<AstNode>),      // port, mask
    DefFn(String, Vec<String>, Box<AstNode>), // name, params, expression
    DefStr(String, String),                 // start_letter, end_letter
    DefInt(String, String),                 // start_letter, end_letter
    DefSng(String, String),                 // start_letter, end_letter
    DefDbl(String, String),                 // start_letter, end_letter
    OptionBase(u8),                         // 0 or 1
    Key(Box<AstNode>, String),              // key_number, string
    KeyOn,
    KeyOff,
    KeyList,
    OnKey(Box<AstNode>, u32),               // key_number, line_number
    DefSeg(Option<Box<AstNode>>),           // optional segment
    Bload(String, Option<Box<AstNode>>),    // filename, optional offset
    Bsave(String, Box<AstNode>, Box<AstNode>), // filename, offset, length
    Call(Box<AstNode>, Vec<AstNode>),       // address, parameters
    Usr(Box<AstNode>),                      // address
    
    // Expressions
    Literal(Value),
    Variable(String),
    BinaryOp(BinaryOperator, Box<AstNode>, Box<AstNode>),
    UnaryOp(UnaryOperator, Box<AstNode>),
    FunctionCall(String, Vec<AstNode>),
    
    // Program structure
    Line(u32, Vec<AstNode>),
    Program(Vec<AstNode>),
}

/// Binary operators
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    IntDivide,
    Mod,
    Power,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    Xor,
    Eqv,
    Imp,
}

/// Unary operators
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

/// Parser that converts tokens into an AST
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    /// Create a new parser from a vector of tokens
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, position: 0 }
    }

    /// Parse the entire program
    pub fn parse(&mut self) -> Result<AstNode> {
        let mut lines = Vec::new();

        while !self.is_at_end() {
            if let TokenType::Eof = self.current_token().token_type {
                break;
            }
            lines.push(self.parse_line()?);
        }

        Ok(AstNode::Program(lines))
    }

    fn parse_line(&mut self) -> Result<AstNode> {
        let line_number = if let TokenType::LineNumber(num) = self.current_token().token_type {
            self.advance();
            Some(num)
        } else {
            None
        };

        let statements = self.parse_statements()?;

        if let Some(num) = line_number {
            Ok(AstNode::Line(num, statements))
        } else {
            // Direct mode - just return the statements
            if statements.len() == 1 {
                Ok(statements[0].clone())
            } else {
                Ok(AstNode::Program(statements))
            }
        }
    }

    fn parse_statements(&mut self) -> Result<Vec<AstNode>> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            match &self.current_token().token_type {
                TokenType::Eof => break,
                TokenType::Newline => {
                    self.advance();
                    // If we have statements, return them (end of line)
                    if !statements.is_empty() {
                        break;
                    }
                    // Otherwise, skip empty lines
                    continue;
                }
                TokenType::LineNumber(_) => {
                    // Stop when we hit a new line number
                    break;
                }
                TokenType::Colon => {
                    self.advance();
                    continue;
                }
                TokenType::Else => {
                    // Stop before ELSE, let the caller handle it
                    break;
                }
                _ => {
                    statements.push(self.parse_statement()?);
                }
            }
        }

        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<AstNode> {
        match &self.current_token().token_type {
            // Basic I/O
            TokenType::Print => self.parse_print(),
            TokenType::Input => self.parse_input(),
            TokenType::Let => self.parse_let(),
            
            // Control Flow
            TokenType::If => self.parse_if(),
            TokenType::For => self.parse_for(),
            TokenType::Next => self.parse_next(),
            TokenType::While => self.parse_while(),
            TokenType::Wend => {
                self.advance();
                Ok(AstNode::Wend)
            }
            TokenType::Goto => self.parse_goto(),
            TokenType::Gosub => self.parse_gosub(),
            TokenType::Return => {
                self.advance();
                Ok(AstNode::Return)
            }
            TokenType::End => {
                self.advance();
                Ok(AstNode::End)
            }
            TokenType::Stop => {
                self.advance();
                Ok(AstNode::Stop)
            }
            TokenType::List => {
                self.advance();
                // Parse optional line range
                let start = if let TokenType::Integer(n) = self.current_token().token_type {
                    self.advance();
                    Some(n as u32)
                } else {
                    None
                };
                let end = if let TokenType::Minus = self.current_token().token_type {
                    self.advance();
                    if let TokenType::Integer(n) = self.current_token().token_type {
                        self.advance();
                        Some(n as u32)
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(AstNode::List(start, end))
            }
            TokenType::New => {
                self.advance();
                Ok(AstNode::New)
            }
            TokenType::Run => {
                self.advance();
                let start_line = if let TokenType::Integer(n) = self.current_token().token_type {
                    self.advance();
                    Some(n as u32)
                } else {
                    None
                };
                Ok(AstNode::Run(start_line))
            }
            TokenType::Write => {
                self.advance();
                // Check if it's WRITE# (file output)
                if let TokenType::Hash = self.current_token().token_type {
                    self.advance();
                    let file_num = self.parse_expression()?;
                    
                    if let TokenType::Comma = self.current_token().token_type {
                        self.advance();
                    }
                    
                    let mut expressions = Vec::new();
                    while !self.is_at_end() {
                        match &self.current_token().token_type {
                            TokenType::Eof | TokenType::Newline | TokenType::Colon => break,
                            TokenType::Comma => {
                                self.advance();
                            }
                            _ => {
                                expressions.push(self.parse_expression()?);
                            }
                        }
                    }
                    
                    Ok(AstNode::WriteFile(Box::new(file_num), expressions))
                } else {
                    // Regular WRITE (to screen)
                    let mut expressions = Vec::new();
                    while !self.is_at_end() {
                        match &self.current_token().token_type {
                            TokenType::Eof | TokenType::Newline | TokenType::Colon => break,
                            TokenType::Comma => {
                                self.advance();
                            }
                            _ => {
                                expressions.push(self.parse_expression()?);
                            }
                        }
                    }
                    
                    Ok(AstNode::PrintFile(Box::new(AstNode::Literal(Value::Integer(0))), expressions))
                }
            }
            TokenType::On => {
                self.advance();
                let expr = self.parse_expression()?;
                
                // Check for GOTO or GOSUB
                let is_goto = if let TokenType::Goto = self.current_token().token_type {
                    self.advance();
                    true
                } else if let TokenType::Gosub = self.current_token().token_type {
                    self.advance();
                    false
                } else {
                    return Err(Error::SyntaxError("Expected GOTO or GOSUB after ON".to_string()));
                };
                
                // Parse line numbers
                let mut lines = vec![];
                loop {
                    if let TokenType::Integer(n) = self.current_token().token_type {
                        lines.push(n as u32);
                        self.advance();
                        if let TokenType::Comma = self.current_token().token_type {
                            self.advance();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                
                if is_goto {
                    Ok(AstNode::OnGoto(Box::new(expr), lines))
                } else {
                    Ok(AstNode::OnGosub(Box::new(expr), lines))
                }
            }
            
            // Data
            TokenType::Dim => self.parse_dim(),
            TokenType::Rem => self.parse_rem(),
            TokenType::Read => {
                self.advance();
                // Simplified READ - just parse variable names
                let mut vars = vec![];
                while !self.is_at_end() {
                    if let TokenType::Identifier(name) = &self.current_token().token_type {
                        vars.push(name.clone());
                        self.advance();
                        if let TokenType::Comma = self.current_token().token_type {
                            self.advance();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Ok(AstNode::Read(vars))
            }
            TokenType::Data => {
                self.advance();
                // DATA statement - store literals
                let mut values = vec![];
                while !self.is_at_end() {
                    match &self.current_token().token_type {
                        TokenType::Newline | TokenType::Colon | TokenType::Eof => break,
                        _ => {
                            values.push(self.parse_expression()?);
                            if let TokenType::Comma = self.current_token().token_type {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                }
                Ok(AstNode::Data(values))
            }
            TokenType::Restore => {
                self.advance();
                let line = if let TokenType::Integer(n) = self.current_token().token_type {
                    self.advance();
                    Some(n as u32)
                } else {
                    None
                };
                Ok(AstNode::Restore(line))
            }
            
            // Screen/Graphics
            TokenType::Cls => {
                self.advance();
                Ok(AstNode::Cls)
            }
            TokenType::Locate => {
                self.advance();
                let row = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let col = self.parse_expression()?;
                Ok(AstNode::Locate(Box::new(row), Box::new(col)))
            }
            TokenType::Color => {
                self.advance();
                let fg = if matches!(self.current_token().token_type, TokenType::Comma | TokenType::Newline | TokenType::Eof) {
                    None
                } else {
                    Some(Box::new(self.parse_expression()?))
                };
                let bg = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    if !matches!(self.current_token().token_type, TokenType::Newline | TokenType::Eof) {
                        Some(Box::new(self.parse_expression()?))
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(AstNode::Color(fg, bg))
            }
            TokenType::Screen => {
                self.advance();
                let mode = self.parse_expression()?;
                Ok(AstNode::Screen(Box::new(mode)))
            }
            TokenType::Width => {
                self.advance();
                let width = self.parse_expression()?;
                Ok(AstNode::Width(Box::new(width)))
            }
            TokenType::Pset => {
                self.advance();
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                }
                let x = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let y = self.parse_expression()?;
                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                }
                let color = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                Ok(AstNode::Pset(Box::new(x), Box::new(y), color))
            }
            TokenType::Circle => {
                self.advance();
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                }
                let x = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let y = self.parse_expression()?;
                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                }
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let radius = self.parse_expression()?;
                let color = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                // Optional: start angle
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    let _ = self.parse_expression()?;
                }
                // Optional: end angle
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    let _ = self.parse_expression()?;
                }
                // Optional: aspect ratio
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    let _ = self.parse_expression()?;
                }
                Ok(AstNode::Circle(Box::new(x), Box::new(y), Box::new(radius), color))
            }
            TokenType::Line => {
                self.advance();
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                }
                let x1 = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let y1 = self.parse_expression()?;
                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                }
                // Expect - or Minus token
                if let TokenType::Minus = self.current_token().token_type {
                    self.advance();
                } else {
                    return Err(Error::SyntaxError("Expected '-' in LINE statement".to_string()));
                }
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                }
                let x2 = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let y2 = self.parse_expression()?;
                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                }
                let color = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                Ok(AstNode::DrawLine(Box::new(x1), Box::new(y1), Box::new(x2), Box::new(y2), color))
            }
            TokenType::Paint => {
                self.advance();
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                }
                let x = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let y = self.parse_expression()?;
                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                }
                let paint_color = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                let border_color = if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                Ok(AstNode::Paint(Box::new(x), Box::new(y), paint_color, border_color))
            }
            
            // Sound
            TokenType::Beep => {
                self.advance();
                Ok(AstNode::Beep)
            }
            TokenType::Sound => {
                self.advance();
                let freq = self.parse_expression()?;
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let duration = self.parse_expression()?;
                Ok(AstNode::Sound(Box::new(freq), Box::new(duration)))
            }
            
            // System
            TokenType::Randomize => {
                self.advance();
                // Check for TIMER keyword
                if let TokenType::Timer = self.current_token().token_type {
                    self.advance();
                    // RANDOMIZE TIMER - use current time as seed
                    return Ok(AstNode::Randomize(None));
                }
                let seed = if matches!(self.current_token().token_type, TokenType::Newline | TokenType::Eof | TokenType::Colon) {
                    None
                } else {
                    Some(Box::new(self.parse_expression()?))
                };
                Ok(AstNode::Randomize(seed))
            }
            TokenType::Swap => {
                self.advance();
                let var1 = if let TokenType::Identifier(n) = &self.current_token().token_type {
                    let v = n.clone();
                    self.advance();
                    v
                } else {
                    return Err(Error::SyntaxError("Expected variable name after SWAP".to_string()));
                };
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                }
                let var2 = if let TokenType::Identifier(n) = &self.current_token().token_type {
                    let v = n.clone();
                    self.advance();
                    v
                } else {
                    return Err(Error::SyntaxError("Expected second variable name in SWAP".to_string()));
                };
                Ok(AstNode::Swap(var1, var2))
            }
            
            TokenType::Identifier(_) => {
                // Could be an assignment without LET
                let name = if let TokenType::Identifier(n) = &self.current_token().token_type {
                    n.clone()
                } else {
                    return Err(Error::SyntaxError("Expected identifier".to_string()));
                };
                self.advance();

                if let TokenType::Equal = self.current_token().token_type {
                    self.advance();
                    let expr = self.parse_expression()?;
                    Ok(AstNode::Let(name, Box::new(expr)))
                } else {
                    Err(Error::SyntaxError(format!("Unexpected token after identifier: {:?}", self.current_token().token_type)))
                }
            }
            _ => Err(Error::SyntaxError(format!("Unexpected token: {:?}", self.current_token().token_type))),
        }
    }

    fn parse_print(&mut self) -> Result<AstNode> {
        self.advance(); // Skip PRINT
        
        // Check if it's PRINT# (file output)
        if let TokenType::Hash = self.current_token().token_type {
            self.advance();
            let file_num = self.parse_expression()?;
            
            // Expect comma after file number
            if let TokenType::Comma = self.current_token().token_type {
                self.advance();
            }
            
            let mut expressions = Vec::new();
            while !self.is_at_end() {
                match &self.current_token().token_type {
                    TokenType::Eof | TokenType::Newline | TokenType::Colon | TokenType::Else => break,
                    TokenType::Semicolon | TokenType::Comma => {
                        self.advance();
                    }
                    _ => {
                        expressions.push(self.parse_expression()?);
                    }
                }
            }

            return Ok(AstNode::PrintFile(Box::new(file_num), expressions));
        }
        
        // Regular PRINT
        let mut expressions = Vec::new();

        while !self.is_at_end() {
            match &self.current_token().token_type {
                TokenType::Eof | TokenType::Newline | TokenType::Colon | TokenType::Else => break,
                TokenType::Semicolon | TokenType::Comma => {
                    self.advance();
                }
                _ => {
                    expressions.push(self.parse_expression()?);
                }
            }
        }

        Ok(AstNode::Print(expressions))
    }

    fn parse_let(&mut self) -> Result<AstNode> {
        self.advance(); // Skip LET

        let name = if let TokenType::Identifier(n) = &self.current_token().token_type {
            n.clone()
        } else {
            return Err(Error::SyntaxError("Expected variable name".to_string()));
        };
        self.advance();

        // Check for array indexing: A(5) = 42
        if let TokenType::LeftParen = self.current_token().token_type {
            self.advance();
            let mut indices = vec![];
            loop {
                indices.push(self.parse_expression()?);
                if let TokenType::Comma = self.current_token().token_type {
                    self.advance();
                } else {
                    break;
                }
            }
            if let TokenType::RightParen = self.current_token().token_type {
                self.advance();
            }

            if let TokenType::Equal = self.current_token().token_type {
                self.advance();
            } else {
                return Err(Error::SyntaxError("Expected '=' in array assignment".to_string()));
            }

            let expr = self.parse_expression()?;
            return Ok(AstNode::ArrayAssign(name, indices, Box::new(expr)));
        }

        // Regular variable assignment
        if let TokenType::Equal = self.current_token().token_type {
            self.advance();
        } else {
            return Err(Error::SyntaxError("Expected '=' in LET statement".to_string()));
        }

        let expr = self.parse_expression()?;
        Ok(AstNode::Let(name, Box::new(expr)))
    }

    fn parse_if(&mut self) -> Result<AstNode> {
        self.advance(); // Skip IF

        let condition = self.parse_expression()?;

        if let TokenType::Then = self.current_token().token_type {
            self.advance();
        } else {
            return Err(Error::SyntaxError("Expected THEN after IF condition".to_string()));
        }

        let then_statements = self.parse_statements()?;

        let else_statements = if let TokenType::Else = self.current_token().token_type {
            self.advance();
            Some(self.parse_statements()?)
        } else {
            None
        };

        Ok(AstNode::If(Box::new(condition), then_statements, else_statements))
    }

    fn parse_for(&mut self) -> Result<AstNode> {
        self.advance(); // Skip FOR

        let var = if let TokenType::Identifier(n) = &self.current_token().token_type {
            n.clone()
        } else {
            return Err(Error::SyntaxError("Expected variable after FOR".to_string()));
        };
        self.advance();

        if let TokenType::Equal = self.current_token().token_type {
            self.advance();
        } else {
            return Err(Error::SyntaxError("Expected '=' in FOR statement".to_string()));
        }

        let start = self.parse_expression()?;

        if let TokenType::To = self.current_token().token_type {
            self.advance();
        } else {
            return Err(Error::SyntaxError("Expected TO in FOR statement".to_string()));
        }

        let end = self.parse_expression()?;

        let step = if let TokenType::Step = self.current_token().token_type {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(AstNode::For(var, Box::new(start), Box::new(end), step))
    }

    fn parse_next(&mut self) -> Result<AstNode> {
        self.advance(); // Skip NEXT

        let var = if let TokenType::Identifier(n) = &self.current_token().token_type {
            let v = n.clone();
            self.advance();
            v
        } else {
            String::new()
        };

        Ok(AstNode::Next(var))
    }

    fn parse_while(&mut self) -> Result<AstNode> {
        self.advance(); // Skip WHILE

        let condition = self.parse_expression()?;

        // Like FOR...NEXT, WHILE...WEND works with the interpreter tracking state
        // and jumping back when WEND is encountered
        Ok(AstNode::While(Box::new(condition)))
    }

    fn parse_goto(&mut self) -> Result<AstNode> {
        self.advance(); // Skip GOTO

        if let TokenType::Integer(line) = self.current_token().token_type {
            self.advance();
            Ok(AstNode::Goto(line as u32))
        } else {
            Err(Error::SyntaxError("Expected line number after GOTO".to_string()))
        }
    }

    fn parse_gosub(&mut self) -> Result<AstNode> {
        self.advance(); // Skip GOSUB

        if let TokenType::Integer(line) = self.current_token().token_type {
            self.advance();
            Ok(AstNode::Gosub(line as u32))
        } else {
            Err(Error::SyntaxError("Expected line number after GOSUB".to_string()))
        }
    }

    fn parse_input(&mut self) -> Result<AstNode> {
        self.advance(); // Skip INPUT
        
        // Check if it's INPUT# (file input)
        if let TokenType::Hash = self.current_token().token_type {
            self.advance();
            let file_num = self.parse_expression()?;
            
            // Expect comma after file number
            if let TokenType::Comma = self.current_token().token_type {
                self.advance();
            }
            
            let mut vars = Vec::new();
            while !self.is_at_end() {
                match &self.current_token().token_type {
                    TokenType::Identifier(name) => {
                        vars.push(name.clone());
                        self.advance();

                        if let TokenType::Comma = self.current_token().token_type {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            
            return Ok(AstNode::InputFile(Box::new(file_num), vars));
        }

        // Regular INPUT - check for optional prompt string
        let mut _prompt = None;
        if let TokenType::String(s) = &self.current_token().token_type {
            _prompt = Some(s.clone());
            self.advance();
            // Skip semicolon or comma after prompt
            if matches!(&self.current_token().token_type, TokenType::Semicolon | TokenType::Comma) {
                self.advance();
            }
        }

        let mut vars = Vec::new();
        while !self.is_at_end() {
            match &self.current_token().token_type {
                TokenType::Identifier(name) => {
                    vars.push(name.clone());
                    self.advance();

                    if let TokenType::Comma = self.current_token().token_type {
                        self.advance();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(AstNode::Input(vars))
    }

    fn parse_dim(&mut self) -> Result<AstNode> {
        self.advance(); // Skip DIM

        let name = if let TokenType::Identifier(n) = &self.current_token().token_type {
            n.clone()
        } else {
            return Err(Error::SyntaxError("Expected array name".to_string()));
        };
        self.advance();

        if let TokenType::LeftParen = self.current_token().token_type {
            self.advance();
        } else {
            return Err(Error::SyntaxError("Expected '(' after array name".to_string()));
        }

        let mut dimensions = Vec::new();
        loop {
            dimensions.push(self.parse_expression()?);

            match &self.current_token().token_type {
                TokenType::Comma => self.advance(),
                TokenType::RightParen => {
                    self.advance();
                    break;
                }
                _ => return Err(Error::SyntaxError("Expected ',' or ')' in DIM statement".to_string())),
            }
        }

        Ok(AstNode::Dim(name, dimensions))
    }

    fn parse_rem(&mut self) -> Result<AstNode> {
        self.advance(); // Skip REM

        // The rest of the line is a comment - just store as empty for now
        // In a full implementation, we'd preserve the comment text
        while !self.is_at_end() {
            match &self.current_token().token_type {
                TokenType::Eof | TokenType::Newline => break,
                _ => {
                    self.advance();
                }
            }
        }

        Ok(AstNode::Rem(String::new()))
    }

    fn parse_expression(&mut self) -> Result<AstNode> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<AstNode> {
        let mut left = self.parse_and()?;

        while let TokenType::Or = self.current_token().token_type {
            self.advance();
            let right = self.parse_and()?;
            left = AstNode::BinaryOp(BinaryOperator::Or, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<AstNode> {
        let mut left = self.parse_comparison()?;

        while let TokenType::And = self.current_token().token_type {
            self.advance();
            let right = self.parse_comparison()?;
            left = AstNode::BinaryOp(BinaryOperator::And, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<AstNode> {
        let mut left = self.parse_addition()?;

        loop {
            let op = match &self.current_token().token_type {
                TokenType::Equal => BinaryOperator::Equal,
                TokenType::NotEqual => BinaryOperator::NotEqual,
                TokenType::LessThan => BinaryOperator::LessThan,
                TokenType::GreaterThan => BinaryOperator::GreaterThan,
                TokenType::LessEqual => BinaryOperator::LessEqual,
                TokenType::GreaterEqual => BinaryOperator::GreaterEqual,
                _ => break,
            };

            self.advance();
            let right = self.parse_addition()?;
            left = AstNode::BinaryOp(op, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<AstNode> {
        let mut left = self.parse_multiplication()?;

        loop {
            let op = match &self.current_token().token_type {
                TokenType::Plus => BinaryOperator::Add,
                TokenType::Minus => BinaryOperator::Subtract,
                _ => break,
            };

            self.advance();
            let right = self.parse_multiplication()?;
            left = AstNode::BinaryOp(op, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<AstNode> {
        let mut left = self.parse_power()?;

        loop {
            let op = match &self.current_token().token_type {
                TokenType::Multiply => BinaryOperator::Multiply,
                TokenType::Divide => BinaryOperator::Divide,
                TokenType::IntDivide => BinaryOperator::IntDivide,
                TokenType::Mod => BinaryOperator::Mod,
                _ => break,
            };

            self.advance();
            let right = self.parse_power()?;
            left = AstNode::BinaryOp(op, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_power(&mut self) -> Result<AstNode> {
        let mut left = self.parse_unary()?;

        while let TokenType::Power = self.current_token().token_type {
            self.advance();
            let right = self.parse_unary()?;
            left = AstNode::BinaryOp(BinaryOperator::Power, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AstNode> {
        match &self.current_token().token_type {
            TokenType::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(AstNode::UnaryOp(UnaryOperator::Negate, Box::new(expr)))
            }
            TokenType::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(AstNode::UnaryOp(UnaryOperator::Not, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<AstNode> {
        match &self.current_token().token_type.clone() {
            TokenType::Integer(val) => {
                let node = AstNode::Literal(Value::Integer(*val));
                self.advance();
                Ok(node)
            }
            TokenType::Float(val) => {
                let node = AstNode::Literal(Value::Double(*val));
                self.advance();
                Ok(node)
            }
            TokenType::String(s) => {
                let node = AstNode::Literal(Value::String(s.clone()));
                self.advance();
                Ok(node)
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();

                // Check for function call
                if let TokenType::LeftParen = self.current_token().token_type {
                    self.advance();
                    let mut args = Vec::new();

                    if let TokenType::RightParen = self.current_token().token_type {
                        self.advance();
                        return Ok(AstNode::FunctionCall(name, args));
                    }

                    loop {
                        args.push(self.parse_expression()?);

                        match &self.current_token().token_type {
                            TokenType::Comma => self.advance(),
                            TokenType::RightParen => {
                                self.advance();
                                break;
                            }
                            _ => return Err(Error::SyntaxError("Expected ',' or ')' in function call".to_string())),
                        }
                    }

                    Ok(AstNode::FunctionCall(name, args))
                } else {
                    Ok(AstNode::Variable(name))
                }
            }
            TokenType::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;

                if let TokenType::RightParen = self.current_token().token_type {
                    self.advance();
                    Ok(expr)
                } else {
                    Err(Error::SyntaxError("Expected ')' after expression".to_string()))
                }
            }
            _ => Err(Error::SyntaxError(format!("Unexpected token in expression: {:?}", self.current_token().token_type))),
        }
    }

    fn current_token(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn test_parser_initialization() {
        let tokens = vec![Token::new(TokenType::Eof, 1, 1)];
        let _parser = Parser::new(tokens);
        assert!(true);
    }

    #[test]
    fn test_parse_print_statement() {
        let mut lexer = Lexer::new("PRINT 42");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Print(exprs) => {
                        assert_eq!(exprs.len(), 1);
                    }
                    _ => panic!("Expected Print node"),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_let_statement() {
        let mut lexer = Lexer::new("LET A = 10");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Let(name, _) => {
                        assert_eq!(name, "A");
                    }
                    _ => panic!("Expected Let node"),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_expression() {
        let mut lexer = Lexer::new("PRINT 1 + 2 * 3");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Print(exprs) => {
                        assert_eq!(exprs.len(), 1);
                        // Should be a binary operation
                        match &exprs[0] {
                            AstNode::BinaryOp(BinaryOperator::Add, _, _) => {},
                            _ => panic!("Expected binary operation"),
                        }
                    }
                    _ => panic!("Expected Print node"),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_line_with_number() {
        let mut lexer = Lexer::new("10 PRINT 42");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Line(num, _) => {
                        assert_eq!(*num, 10);
                    }
                    _ => panic!("Expected Line node"),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_dim_statement() {
        let mut lexer = Lexer::new("DIM A(10)");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Dim(name, dims) => {
                        assert_eq!(name, "A");
                        assert_eq!(dims.len(), 1);
                    }
                    _ => panic!("Expected Dim node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_array_assignment() {
        let mut lexer = Lexer::new("LET A(5) = 42");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::ArrayAssign(name, indices, _) => {
                        assert_eq!(name, "A");
                        assert_eq!(indices.len(), 1);
                    }
                    _ => panic!("Expected ArrayAssign node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_for_loop() {
        let mut lexer = Lexer::new("FOR I = 1 TO 10");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::For(var, _, _, _) => {
                        assert_eq!(var, "I");
                    }
                    _ => panic!("Expected For node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_if_then() {
        let mut lexer = Lexer::new("IF X > 5 THEN PRINT \"Big\"");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::If(_, then_stmts, else_stmts) => {
                        assert!(then_stmts.len() > 0);
                        assert!(else_stmts.is_none());
                    }
                    _ => panic!("Expected If node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_if_then_else() {
        let mut lexer = Lexer::new("IF X > 5 THEN PRINT \"Big\" ELSE PRINT \"Small\"");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::If(_, then_stmts, else_stmts) => {
                        assert!(then_stmts.len() > 0);
                        assert!(else_stmts.is_some());
                    }
                    _ => panic!("Expected If node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_input_with_prompt() {
        let mut lexer = Lexer::new("INPUT \"Enter value\"; X");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Input(vars) => {
                        assert_eq!(vars.len(), 1);
                        assert_eq!(vars[0], "X");
                    }
                    _ => panic!("Expected Input node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_randomize_timer() {
        let mut lexer = Lexer::new("RANDOMIZE TIMER");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Randomize(_) => {
                        // Success
                    }
                    _ => panic!("Expected Randomize node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_gosub_return() {
        let mut lexer = Lexer::new("GOSUB 100");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Gosub(line) => {
                        assert_eq!(*line, 100);
                    }
                    _ => panic!("Expected Gosub node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }

    #[test]
    fn test_parse_goto() {
        let mut lexer = Lexer::new("GOTO 200");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        match ast {
            AstNode::Program(lines) => {
                assert_eq!(lines.len(), 1);
                match &lines[0] {
                    AstNode::Goto(line) => {
                        assert_eq!(*line, 200);
                    }
                    _ => panic!("Expected Goto node, got {:?}", lines[0]),
                }
            }
            _ => panic!("Expected Program node"),
        }
    }
}