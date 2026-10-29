enum TokenKind {
    LParen,
    RParen,
    Word,              // bareword OR quoted string; `escaped` distinguishes them
}

struct Token {
    kind: TokenKind,
    value: String,     // empty for LParen/RParen, decoded text for Word
    line: u32,
    col: u32,
    escaped: bool,     // true if this Word came from a "..." literal
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    
    fn new(input: &'a str) -> Self {
        Lexer { input, pos: 0, line: 1, col: 1 }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, SaifError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace_and_comments()?;

            let (line, col) = (self.line, self.col);   // token's start position

            match self.peek_char() {
                None => break,                           // EOF, done

                Some('(') => {
                    self.advance_char(); // I am running peek twice, should consider...
                    tokens.push(Token { kind: TokenKind::LParen, value: String::new(), line, col, escaped: false });
                }
                Some(')') => {
                    self.advance_char();
                    tokens.push(Token { kind: TokenKind::RParen, value: String::new(), line, col, escaped: false });
                }
                Some('"') => {
                    tokens.push(self.read_quoted_string()?);   // constructs the Token itself
                }
                Some(_) => {
                    tokens.push(self.read_word());             // constructs the Token itself
                }
            }
        }

        return Ok(tokens)
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;   // early-return None if at EOF, c is `char` from here on

        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.pos += c.len_utf8();    // pos still needs the byte width, for the UTF-8 slicing in peek_char

        return Some(c)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), SaifError> { // handles `//...`, `/* ... */`
        loop {
            match self.peek_char() {
                Some(c) if c.is_whitespace() => {
                    self.advance_char();
                }
                Some('/') if self.input[self.pos..].chars().nth(1) == Some('/') => {
                    // line comment: skip to end of line (or EOF)
                    self.advance_char(); // first '/'
                    self.advance_char(); // second '/'
                    while let Some(c) = self.peek_char() {
                        if c == '\n' {
                            break;
                        }
                        self.advance_char();
                    }
                }
                Some('/') if self.input[self.pos..].chars().nth(1) == Some('*') => {
                    // block comment: skip to the matching '*/'
                    let start_line = self.line;
                    let start_col = self.col;
                    self.advance_char(); // '/'
                    self.advance_char(); // '*'
                    loop {
                        match self.peek_char() {
                            None => {
                                return Err(SaifError {
                                    line: start_line,
                                    col: start_col,
                                    kind: SaifErrorKind::UnterminatedBlockComment,
                                });
                            }
                            Some('*') if self.input[self.pos..].chars().nth(1) == Some('/') => {
                                self.advance_char(); // '*'
                                self.advance_char(); // '/'
                                break;
                            }
                            Some(_) => {
                                self.advance_char();
                            }
                        }
                    }
                }
                // A lone '/' that isn't starting a comment (e.g. the DIVIDER
                // value in `(DIVIDER / )`) is left untouched for read_word.
                _ => break,
            }
        }

        Ok(())
    }

    fn read_word(&mut self) -> Token { // bareword, unescaped
        let start_line = self.line;
        let start_col = self.col;

        let decoded_text: String = self.input[self.pos..]
            .chars()
            .take_while(|&c| c != ' ' && c != '\n' && c != '(' && c != ')')
            .collect();

        self.pos += decoded_text.len();                    // byte length consumed
        self.col += decoded_text.chars().count() as u32;    // character count consumed

        Token {
            kind: TokenKind::Word,
            value: decoded_text,
            line: start_line,
            col: start_col,
            escaped: false,
        }
    }

    fn read_quoted_string(&mut self) -> Result<Token, SaifError> {
        // Record starting point: the opening quote's position.
        let start_line = self.line;
        let start_col = self.col;

        self.advance_char(); // consume the opening quote

        let decoded_text: String = self.input[self.pos..]
            .chars()
            .take_while(|&c| c != '"')
            .collect();

        self.pos += decoded_text.len();
        self.col += decoded_text.chars().count() as u32;

        // take_while only stops on '"' or EOF -- if advance_char doesn't find
        // the closing quote here, the string ran off the end of the file.
        match self.advance_char() {
            Some('"') => {}
            _ => {
                return Err(SaifError {
                    line: start_line,
                    col: start_col,
                    kind: SaifErrorKind::UnterminatedString,
                });
            }
        }

        Ok(Token {
            kind: TokenKind::Word,
            value: decoded_text,
            line: start_line,
            col: start_col,
            escaped: true,
        })
    }
}

// AST types
struct SaifHeader {
    direction: Option<String>,
    saif_version: Option<String>,
    design: Option<String>,
    date: Option<String>,
    vendor: Option<String>,
    program_name: Option<String>,
    version: Option<String>,
    divider: Option<String>,
    timescale: Option<String>,
    duration: Option<String>,
}

struct SaifNet {
    name: String,
    counts: Vec<(String, u64)>,   // TC/IG/T0/T1/TX/TZ/TB/TG/IK, generic
}

struct SaifInstance {
    name: String,
    nets: Vec<SaifNet>,
    children: Vec<SaifInstance>,
}

struct SaifFile {
    header: SaifHeader,
    top_instances: Vec<SaifInstance>,
}

// Parser
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self;

    // cursor primitives
    fn peek(&self) -> Option<&Token>;
    fn advance(&mut self) -> Option<Token>;
    fn expect_lparen(&mut self) -> Result<(), SaifError>;
    fn expect_rparen(&mut self) -> Result<(), SaifError>;
    fn expect_word(&mut self) -> Result<Token, SaifError>;
    fn expect_keyword(&mut self, kw: &str) -> Result<(), SaifError>;

    // grammar — recursive descent, one fn per nonterminal
    fn parse_file(&mut self) -> Result<SaifFile, SaifError>;
    fn parse_header_field(&mut self, header: &mut SaifHeader, keyword: &str) -> Result<(), SaifError>;
    fn parse_instance(&mut self) -> Result<SaifInstance, SaifError>;   // recurses into itself
    fn parse_net_list(&mut self) -> Result<Vec<SaifNet>, SaifError>;
    fn parse_net(&mut self) -> Result<SaifNet, SaifError>;
}

// Top-level entry point
pub fn parse(input: &str) -> Result<SaifFile, SaifError>;
//   = Lexer::new(input).tokenize().and_then(|t| Parser::new(t).parse_file())

// Errors
struct SaifError {
    line: u32,
    col: u32,
    kind: SaifErrorKind,
}

enum SaifErrorKind {
    UnterminatedString,
    UnterminatedBlockComment,
    UnexpectedToken { expected: &'static str, found: String },
    UnexpectedEof,
    UnknownHeaderField(String),
    InvalidCount(String),   // a TC/T0/.../value token that didn't parse as u64
}
