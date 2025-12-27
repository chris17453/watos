//! Lexical analyzer for GW-BASIC

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec, format, string::ToString};

use crate::error::{Error, Result};

/// Token types in GW-BASIC
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Literals
    Integer(i32),
    Float(f64),
    String(String),
    
    // Keywords - Control Flow
    Print,
    Let,
    If,
    Then,
    Else,
    For,
    Next,
    To,
    Step,
    While,
    Wend,
    Goto,
    Gosub,
    Return,
    End,
    Stop,
    Cont,
    
    // Keywords - I/O
    Input,
    LineInput,
    Write,
    Open,
    Close,
    Load,
    Save,
    Run,
    List,
    New,
    
    // Keywords - Data
    Dim,
    Rem,
    Data,
    Read,
    Restore,
    Defstr,
    Defint,
    Defsng,
    Defdbl,
    
    // Keywords - Array/Memory
    Erase,
    Clear,
    Swap,
    
    // Keywords - Screen/Graphics
    Cls,
    Locate,
    Color,
    Screen,
    Width,
    View,
    Window,
    Pset,
    Preset,
    Line,
    Circle,
    Paint,
    Draw,
    Get,
    Put,
    
    // Keywords - Sound
    Beep,
    Sound,
    Play,
    
    // Keywords - System
    Key,
    On,
    Off,
    Wait,
    Randomize,
    Timer,
    Date,
    Time,
    Poke,
    Peek,
    Out,
    Inp,
    Call,
    Usr,
    Bload,
    Bsave,
    Seg,
    Option,
    Base,
    Palette,
    
    // Keywords - File Operations
    Files,
    Kill,
    Name,
    Merge,
    Chain,
    Field,
    Lset,
    Rset,
    Reset,
    Using,
    As,
    ForMode,   // FOR in OPEN statement
    Append,
    Random,
    Output,
    Binary,
    
    // Keywords - Error Handling
    Error,
    Resume,
    
    // Keywords - Functions (can also be identifiers)
    Def,
    Fn,
    
    // Keywords - Program Control
    Auto,
    Delete,
    Renum,
    Edit,
    Tron,
    Troff,
    
    // Operators
    Plus,
    Minus,
    Multiply,
    Divide,
    IntDivide,     // Backslash
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
    Not,
    Xor,
    Eqv,
    Imp,
    
    // Delimiters
    LeftParen,
    RightParen,
    Comma,
    Colon,
    Semicolon,
    Dollar,        // For string variables
    Percent,       // For integer variables
    Exclamation,   // For single precision
    Hash,          // For double precision or file numbers
    
    // Other
    Identifier(String),
    LineNumber(u32),
    Newline,
    Eof,
}

/// Represents a token with its type and position
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(token_type: TokenType, line: usize, column: usize) -> Self {
        Token { token_type, line, column }
    }
}

/// Lexical analyzer that converts source code into tokens
pub struct Lexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    /// Create a new lexer from source code
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    /// Get the next token from the input
    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();

        if self.is_at_end() {
            return Ok(Token::new(TokenType::Eof, self.line, self.column));
        }

        let start_line = self.line;
        let start_column = self.column;
        let ch = self.current_char();

        // Line numbers at start of line
        if self.column == 1 && ch.is_ascii_digit() {
            return self.read_line_number();
        }

        // Numbers
        if ch.is_ascii_digit() {
            return self.read_number();
        }

        // Strings
        if ch == '"' {
            return self.read_string();
        }

        // Identifiers and keywords
        if ch.is_alphabetic() {
            return self.read_identifier();
        }

        // Operators and delimiters
        let token_type = match ch {
            '+' => { self.advance(); TokenType::Plus }
            '-' => { self.advance(); TokenType::Minus }
            '*' => { self.advance(); TokenType::Multiply }
            '/' => { self.advance(); TokenType::Divide }
            '\\' => { self.advance(); TokenType::IntDivide }
            '^' => { self.advance(); TokenType::Power }
            '(' => { self.advance(); TokenType::LeftParen }
            ')' => { self.advance(); TokenType::RightParen }
            ',' => { self.advance(); TokenType::Comma }
            ':' => { self.advance(); TokenType::Colon }
            ';' => { self.advance(); TokenType::Semicolon }
            '$' => { self.advance(); TokenType::Dollar }
            '%' => { self.advance(); TokenType::Percent }
            '!' => { self.advance(); TokenType::Exclamation }
            '#' => { self.advance(); TokenType::Hash }
            '=' => { self.advance(); TokenType::Equal }
            '<' => {
                self.advance();
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    TokenType::LessEqual
                } else if !self.is_at_end() && self.current_char() == '>' {
                    self.advance();
                    TokenType::NotEqual
                } else {
                    TokenType::LessThan
                }
            }
            '>' => {
                self.advance();
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    TokenType::GreaterEqual
                } else {
                    TokenType::GreaterThan
                }
            }
            '\n' => {
                let token = Token::new(TokenType::Newline, self.line, self.column);
                self.advance();
                self.line += 1;
                self.column = 1;
                return Ok(token);
            }
            _ => {
                return Err(Error::SyntaxError(format!("Unexpected character: '{}'", ch)));
            }
        };

        Ok(Token::new(token_type, start_line, start_column))
    }

    fn current_char(&self) -> char {
        self.input[self.position]
    }

    fn advance(&mut self) {
        self.position += 1;
        self.column += 1;
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            let ch = self.current_char();
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_line_number(&mut self) -> Result<Token> {
        let start_line = self.line;
        let start_column = self.column;
        let mut num_str = String::new();

        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            num_str.push(self.current_char());
            self.advance();
        }

        let num: u32 = num_str.parse()
            .map_err(|_| Error::SyntaxError(format!("Invalid line number: {}", num_str)))?;

        Ok(Token::new(TokenType::LineNumber(num), start_line, start_column))
    }

    fn read_number(&mut self) -> Result<Token> {
        let start_line = self.line;
        let start_column = self.column;
        let mut num_str = String::new();
        let mut is_float = false;

        while !self.is_at_end() {
            let ch = self.current_char();
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                is_float = true;
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let token_type = if is_float {
            let val: f64 = num_str.parse()
                .map_err(|_| Error::SyntaxError(format!("Invalid float: {}", num_str)))?;
            TokenType::Float(val)
        } else {
            let val: i32 = num_str.parse()
                .map_err(|_| Error::SyntaxError(format!("Invalid integer: {}", num_str)))?;
            TokenType::Integer(val)
        };

        Ok(Token::new(token_type, start_line, start_column))
    }

    fn read_string(&mut self) -> Result<Token> {
        let start_line = self.line;
        let start_column = self.column;
        
        self.advance(); // Skip opening quote
        let mut string = String::new();

        while !self.is_at_end() && self.current_char() != '"' {
            string.push(self.current_char());
            self.advance();
        }

        if self.is_at_end() {
            return Err(Error::SyntaxError("Unterminated string".to_string()));
        }

        self.advance(); // Skip closing quote
        Ok(Token::new(TokenType::String(string), start_line, start_column))
    }

    fn read_identifier(&mut self) -> Result<Token> {
        let start_line = self.line;
        let start_column = self.column;
        let mut ident = String::new();

        while !self.is_at_end() {
            let ch = self.current_char();
            if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '%' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let token_type = match ident.to_uppercase().as_str() {
            // Control Flow
            "PRINT" => TokenType::Print,
            "LET" => TokenType::Let,
            "IF" => TokenType::If,
            "THEN" => TokenType::Then,
            "ELSE" => TokenType::Else,
            "FOR" => TokenType::For,
            "NEXT" => TokenType::Next,
            "TO" => TokenType::To,
            "STEP" => TokenType::Step,
            "WHILE" => TokenType::While,
            "WEND" => TokenType::Wend,
            "GOTO" => TokenType::Goto,
            "GOSUB" => TokenType::Gosub,
            "RETURN" => TokenType::Return,
            "END" => TokenType::End,
            "STOP" => TokenType::Stop,
            "CONT" => TokenType::Cont,
            
            // I/O
            "INPUT" => TokenType::Input,
            "WRITE" => TokenType::Write,
            "LINE" => TokenType::Line,
            "OPEN" => TokenType::Open,
            "CLOSE" => TokenType::Close,
            "LOAD" => TokenType::Load,
            "SAVE" => TokenType::Save,
            "RUN" => TokenType::Run,
            "LIST" => TokenType::List,
            "NEW" => TokenType::New,
            
            // Data
            "DIM" => TokenType::Dim,
            "REM" => TokenType::Rem,
            "DATA" => TokenType::Data,
            "READ" => TokenType::Read,
            "RESTORE" => TokenType::Restore,
            "DEFSTR" => TokenType::Defstr,
            "DEFINT" => TokenType::Defint,
            "DEFSNG" => TokenType::Defsng,
            "DEFDBL" => TokenType::Defdbl,
            
            // Array/Memory
            "ERASE" => TokenType::Erase,
            "CLEAR" => TokenType::Clear,
            "SWAP" => TokenType::Swap,
            
            // Screen/Graphics
            "CLS" => TokenType::Cls,
            "LOCATE" => TokenType::Locate,
            "COLOR" => TokenType::Color,
            "SCREEN" => TokenType::Screen,
            "WIDTH" => TokenType::Width,
            "VIEW" => TokenType::View,
            "WINDOW" => TokenType::Window,
            "PSET" => TokenType::Pset,
            "PRESET" => TokenType::Preset,
            "CIRCLE" => TokenType::Circle,
            "PAINT" => TokenType::Paint,
            "DRAW" => TokenType::Draw,
            "GET" => TokenType::Get,
            "PUT" => TokenType::Put,
            
            // Sound
            "BEEP" => TokenType::Beep,
            "SOUND" => TokenType::Sound,
            "PLAY" => TokenType::Play,
            
            // System
            "KEY" => TokenType::Key,
            "ON" => TokenType::On,
            "OFF" => TokenType::Off,
            "WAIT" => TokenType::Wait,
            "RANDOMIZE" => TokenType::Randomize,
            "TIMER" => TokenType::Timer,
            "DATE" => TokenType::Date,
            "TIME" => TokenType::Time,
            "POKE" => TokenType::Poke,
            "PEEK" => TokenType::Peek,
            "OUT" => TokenType::Out,
            "INP" => TokenType::Inp,
            "CALL" => TokenType::Call,
            "USR" => TokenType::Usr,
            "BLOAD" => TokenType::Bload,
            "BSAVE" => TokenType::Bsave,
            "SEG" => TokenType::Seg,
            "OPTION" => TokenType::Option,
            "BASE" => TokenType::Base,
            "PALETTE" => TokenType::Palette,
            
            // File Operations
            "FILES" => TokenType::Files,
            "KILL" => TokenType::Kill,
            "NAME" => TokenType::Name,
            "MERGE" => TokenType::Merge,
            "CHAIN" => TokenType::Chain,
            "FIELD" => TokenType::Field,
            "LSET" => TokenType::Lset,
            "RSET" => TokenType::Rset,
            "RESET" => TokenType::Reset,
            "USING" => TokenType::Using,
            "AS" => TokenType::As,
            "APPEND" => TokenType::Append,
            "RANDOM" => TokenType::Random,
            "OUTPUT" => TokenType::Output,
            "BINARY" => TokenType::Binary,
            
            // Error Handling
            "ERROR" => TokenType::Error,
            "RESUME" => TokenType::Resume,
            
            // Functions
            "DEF" => TokenType::Def,
            "FN" => TokenType::Fn,
            
            // Program Control
            "AUTO" => TokenType::Auto,
            "DELETE" => TokenType::Delete,
            "RENUM" => TokenType::Renum,
            "EDIT" => TokenType::Edit,
            "TRON" => TokenType::Tron,
            "TROFF" => TokenType::Troff,
            
            // Logical Operators
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "NOT" => TokenType::Not,
            "XOR" => TokenType::Xor,
            "EQV" => TokenType::Eqv,
            "IMP" => TokenType::Imp,
            "MOD" => TokenType::Mod,
            
            _ => TokenType::Identifier(ident),
        };

        Ok(Token::new(token_type, start_line, start_column))
    }

    /// Tokenize entire input into a vector of tokens
    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.token_type == TokenType::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_initialization() {
        let lexer = Lexer::new("PRINT 42");
        assert_eq!(lexer.position, 0);
        assert_eq!(lexer.line, 1);
    }

    #[test]
    fn test_tokenize_print_statement() {
        let mut lexer = Lexer::new("PRINT 42");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3); // PRINT, 42, EOF
        assert_eq!(tokens[0].token_type, TokenType::Print);
        assert_eq!(tokens[1].token_type, TokenType::Integer(42));
        assert_eq!(tokens[2].token_type, TokenType::Eof);
    }

    #[test]
    fn test_tokenize_string() {
        let mut lexer = Lexer::new(r#"PRINT "Hello""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[1].token_type, TokenType::String("Hello".to_string()));
    }

    #[test]
    fn test_tokenize_operators() {
        let mut lexer = Lexer::new("A = 1 + 2 * 3");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::Identifier("A".to_string()));
        assert_eq!(tokens[1].token_type, TokenType::Equal);
        assert_eq!(tokens[2].token_type, TokenType::Integer(1));
        assert_eq!(tokens[3].token_type, TokenType::Plus);
        assert_eq!(tokens[4].token_type, TokenType::Integer(2));
        assert_eq!(tokens[5].token_type, TokenType::Multiply);
        assert_eq!(tokens[6].token_type, TokenType::Integer(3));
    }

    #[test]
    fn test_tokenize_comparison() {
        let mut lexer = Lexer::new("A <= B");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[1].token_type, TokenType::LessEqual);
    }

    #[test]
    fn test_line_number() {
        let mut lexer = Lexer::new("10 PRINT");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].token_type, TokenType::LineNumber(10));
        assert_eq!(tokens[1].token_type, TokenType::Print);
    }
}