//! ANSI escape sequence parser
//!
//! State machine that processes bytes and emits terminal events.
//! Handles VT100/VT220/xterm sequences.

/// Maximum number of CSI parameters
const MAX_PARAMS: usize = 16;

/// Maximum length of OSC string
const MAX_OSC_LEN: usize = 256;

/// Parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Normal text processing
    Ground,
    /// Received ESC
    Escape,
    /// Received ESC [
    CsiEntry,
    /// Collecting CSI parameters
    CsiParam,
    /// Received ESC [ ? (DEC private)
    CsiPrivate,
    /// Received ESC ]
    OscString,
    /// Received ESC ( or ESC )
    CharsetSelect,
}

/// Events produced by the parser
#[derive(Debug, Clone)]
pub enum Event {
    /// Print a character
    Print(char),
    /// Execute a control character (C0/C1)
    Execute(u8),
    /// CSI sequence: command and parameters
    Csi {
        params: [i32; MAX_PARAMS],
        param_count: usize,
        intermediate: u8,
        final_byte: u8,
        private: bool,
    },
    /// OSC sequence (operating system command)
    Osc {
        command: u8,
    },
    /// ESC single-character command
    EscDispatch(u8),
    /// Charset selection
    Charset {
        slot: u8,  // '(' or ')'
        charset: u8,
    },
}

/// ANSI escape sequence parser
pub struct Parser {
    state: State,
    params: [i32; MAX_PARAMS],
    param_count: usize,
    current_param: i32,
    intermediate: u8,
    private: bool,
    charset_slot: u8,
    osc_cmd: u8,
    osc_len: usize,
}

impl Parser {
    /// Create a new parser
    pub const fn new() -> Self {
        Self {
            state: State::Ground,
            params: [0; MAX_PARAMS],
            param_count: 0,
            current_param: 0,
            intermediate: 0,
            private: false,
            charset_slot: 0,
            osc_cmd: 0,
            osc_len: 0,
        }
    }

    /// Reset parser to ground state
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.params = [0; MAX_PARAMS];
        self.param_count = 0;
        self.current_param = 0;
        self.intermediate = 0;
        self.private = false;
    }

    /// Process a single byte, returning an event if one is produced
    pub fn advance(&mut self, byte: u8) -> Option<Event> {
        match self.state {
            State::Ground => self.ground(byte),
            State::Escape => self.escape(byte),
            State::CsiEntry => self.csi_entry(byte),
            State::CsiParam => self.csi_param(byte),
            State::CsiPrivate => self.csi_private(byte),
            State::OscString => self.osc_string(byte),
            State::CharsetSelect => self.charset_select(byte),
        }
    }

    fn ground(&mut self, byte: u8) -> Option<Event> {
        match byte {
            // C0 control characters
            0x00..=0x1F => {
                if byte == 0x1B {
                    // ESC
                    self.state = State::Escape;
                    None
                } else {
                    Some(Event::Execute(byte))
                }
            }
            // Printable ASCII
            0x20..=0x7E => Some(Event::Print(byte as char)),
            // DEL - ignore
            0x7F => None,
            // C1 control characters (8-bit)
            0x80..=0x9F => {
                match byte {
                    0x9B => {
                        // CSI (equivalent to ESC [)
                        self.enter_csi();
                        None
                    }
                    0x9D => {
                        // OSC (equivalent to ESC ])
                        self.state = State::OscString;
                        self.osc_cmd = 0;
                        self.osc_len = 0;
                        None
                    }
                    _ => Some(Event::Execute(byte)),
                }
            }
            // UTF-8 continuation or start bytes - treat as printable for now
            // TODO: Full UTF-8 decoding
            0xA0..=0xFF => {
                // For now, just show the byte as a character
                // This works for Latin-1 supplement
                Some(Event::Print(byte as char))
            }
        }
    }

    fn escape(&mut self, byte: u8) -> Option<Event> {
        match byte {
            b'[' => {
                self.enter_csi();
                None
            }
            b']' => {
                self.state = State::OscString;
                self.osc_cmd = 0;
                self.osc_len = 0;
                None
            }
            b'(' | b')' => {
                self.state = State::CharsetSelect;
                self.charset_slot = byte;
                None
            }
            // Single-character escape sequences
            b'7' | b'8' | b'=' | b'>' | b'c' | b'D' | b'E' | b'H' | b'M' | b'Z' => {
                self.state = State::Ground;
                Some(Event::EscDispatch(byte))
            }
            // Ignore unknown
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn enter_csi(&mut self) {
        self.state = State::CsiEntry;
        self.params = [0; MAX_PARAMS];
        self.param_count = 0;
        self.current_param = 0;
        self.intermediate = 0;
        self.private = false;
    }

    fn csi_entry(&mut self, byte: u8) -> Option<Event> {
        match byte {
            b'?' => {
                self.private = true;
                self.state = State::CsiPrivate;
                None
            }
            b'0'..=b'9' => {
                self.current_param = (byte - b'0') as i32;
                self.state = State::CsiParam;
                None
            }
            b';' => {
                // Empty first parameter (default 0)
                if self.param_count < MAX_PARAMS {
                    self.params[self.param_count] = 0;
                    self.param_count += 1;
                }
                self.state = State::CsiParam;
                None
            }
            // Final byte
            0x40..=0x7E => {
                self.state = State::Ground;
                Some(Event::Csi {
                    params: self.params,
                    param_count: self.param_count,
                    intermediate: self.intermediate,
                    final_byte: byte,
                    private: false,
                })
            }
            // Intermediate bytes
            0x20..=0x2F => {
                self.intermediate = byte;
                None
            }
            // Cancel
            0x18 | 0x1A => {
                self.state = State::Ground;
                None
            }
            // ESC restarts
            0x1B => {
                self.state = State::Escape;
                None
            }
            _ => None,
        }
    }

    fn csi_param(&mut self, byte: u8) -> Option<Event> {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param * 10 + (byte - b'0') as i32;
                None
            }
            b';' => {
                if self.param_count < MAX_PARAMS {
                    self.params[self.param_count] = self.current_param;
                    self.param_count += 1;
                }
                self.current_param = 0;
                None
            }
            // Final byte
            0x40..=0x7E => {
                // Save last parameter
                if self.param_count < MAX_PARAMS {
                    self.params[self.param_count] = self.current_param;
                    self.param_count += 1;
                }
                self.state = State::Ground;
                Some(Event::Csi {
                    params: self.params,
                    param_count: self.param_count,
                    intermediate: self.intermediate,
                    final_byte: byte,
                    private: self.private,
                })
            }
            // Intermediate bytes
            0x20..=0x2F => {
                self.intermediate = byte;
                None
            }
            // Cancel
            0x18 | 0x1A => {
                self.state = State::Ground;
                None
            }
            // ESC restarts
            0x1B => {
                self.state = State::Escape;
                None
            }
            _ => None,
        }
    }

    fn csi_private(&mut self, byte: u8) -> Option<Event> {
        // Same as csi_param but with private flag set
        self.private = true;
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param * 10 + (byte - b'0') as i32;
                self.state = State::CsiParam;
                None
            }
            b';' => {
                if self.param_count < MAX_PARAMS {
                    self.params[self.param_count] = self.current_param;
                    self.param_count += 1;
                }
                self.current_param = 0;
                self.state = State::CsiParam;
                None
            }
            // Final byte
            0x40..=0x7E => {
                if self.param_count < MAX_PARAMS {
                    self.params[self.param_count] = self.current_param;
                    self.param_count += 1;
                }
                self.state = State::Ground;
                Some(Event::Csi {
                    params: self.params,
                    param_count: self.param_count,
                    intermediate: self.intermediate,
                    final_byte: byte,
                    private: true,
                })
            }
            // Cancel
            0x18 | 0x1A => {
                self.state = State::Ground;
                None
            }
            0x1B => {
                self.state = State::Escape;
                None
            }
            _ => {
                self.state = State::CsiParam;
                None
            }
        }
    }

    fn osc_string(&mut self, byte: u8) -> Option<Event> {
        match byte {
            // String terminator (BEL)
            0x07 => {
                self.state = State::Ground;
                Some(Event::Osc { command: self.osc_cmd })
            }
            // ESC might be start of ST (ESC \)
            0x1B => {
                // For simplicity, treat ESC as terminator
                // Full implementation would check for backslash
                self.state = State::Ground;
                Some(Event::Osc { command: self.osc_cmd })
            }
            // ST (String Terminator, 8-bit)
            0x9C => {
                self.state = State::Ground;
                Some(Event::Osc { command: self.osc_cmd })
            }
            // First character after OSC is the command number
            b'0'..=b'9' if self.osc_len == 0 => {
                self.osc_cmd = byte - b'0';
                self.osc_len += 1;
                None
            }
            // Ignore the rest of the OSC string content
            _ => {
                self.osc_len += 1;
                if self.osc_len > MAX_OSC_LEN {
                    self.state = State::Ground;
                }
                None
            }
        }
    }

    fn charset_select(&mut self, byte: u8) -> Option<Event> {
        self.state = State::Ground;
        Some(Event::Charset {
            slot: self.charset_slot,
            charset: byte,
        })
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

// Helper to get a CSI parameter with default value
pub fn csi_param(params: &[i32; MAX_PARAMS], count: usize, index: usize, default: i32) -> i32 {
    if index < count && params[index] != 0 {
        params[index]
    } else {
        default
    }
}
