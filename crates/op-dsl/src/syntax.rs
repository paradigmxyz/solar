//! The generic syntax shared by every dialect: tokens and a tree of nodes.
//!
//! A file is a list of nodes. A node is a run of head tokens ended by `;`, or by a `{ ... }`
//! body of child nodes. Parentheses and brackets nest inside a head, so a brace inside them never
//! opens a body, and the lexer reads `{..}` as one token so struct patterns can appear anywhere.
//! `///` comments document the node or field that follows them; `//` comments are skipped.
//!
//! Dialect schemas give the nodes meaning: [`Cursor`] walks a head, and every error points at
//! the token that caused it.

use std::fmt;

/// A position in a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Index of the file in the compiled set.
    pub file: usize,
    /// One-based line.
    pub line: u32,
    /// One-based column.
    pub col: u32,
}

/// A diagnostic with the position it refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Where the problem is.
    pub span: Span,
    /// What is wrong.
    pub message: String,
}

impl Error {
    /// Creates an error at `span`.
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self { span, message: message.into() }
    }
}

/// The result of a schema step.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The kind of a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// An identifier or keyword.
    Ident(String),
    /// A decimal or `0x` hexadecimal integer.
    Int(u64),
    /// A double-quoted string, without escapes.
    Str(String),
    /// A punctuation token, such as `->` or `(`.
    Punct(&'static str),
    /// A `///` comment line, without the slashes and the first space.
    Doc(String),
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ident(name) => write!(f, "`{name}`"),
            Self::Int(value) => write!(f, "`{value}`"),
            Self::Str(value) => write!(f, "\"{value}\""),
            Self::Punct(punct) => write!(f, "`{punct}`"),
            Self::Doc(_) => f.write_str("doc comment"),
        }
    }
}

/// A token and its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Punctuation, longest first so the lexer matches greedily.
const PUNCTS: &[&str] = &[
    "{..}", "..=", "->", "==", "!=", "<=", ">=", "&&", "||", "..", "(", ")", "[", "]", "{", "}",
    "<", ">", ",", ";", ":", "=", "|", "!", "?", "+", "-", "*", ".", "#",
];

/// Splits a source file into tokens.
pub fn lex(file: usize, src: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut col = 1;
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let span = Span { file, line, col };
        let rest = &src[i..];
        let c = bytes[i];
        let len = if c == b'\n' {
            line += 1;
            col = 1;
            i += 1;
            continue;
        } else if c.is_ascii_whitespace() {
            1
        } else if let Some(comment) = rest.strip_prefix("///") {
            let end = comment.find('\n').unwrap_or(comment.len());
            let text = &comment[..end];
            tokens.push(Token {
                tok: Tok::Doc(text.strip_prefix(' ').unwrap_or(text).trim_end().to_string()),
                span,
            });
            3 + end
        } else if rest.starts_with("//") {
            rest.find('\n').unwrap_or(rest.len())
        } else if c == b'"' {
            let Some(end) = rest[1..].find('"') else {
                return Err(Error::new(span, "unterminated string"));
            };
            tokens.push(Token { tok: Tok::Str(rest[1..1 + end].to_string()), span });
            end + 2
        } else if c.is_ascii_digit() {
            let end =
                rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_').unwrap_or(rest.len());
            let text = rest[..end].replace('_', "");
            let value = match text.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => text.parse(),
            }
            .map_err(|_| Error::new(span, format!("invalid integer `{}`", &rest[..end])))?;
            tokens.push(Token { tok: Tok::Int(value), span });
            end
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let end =
                rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_').unwrap_or(rest.len());
            tokens.push(Token { tok: Tok::Ident(rest[..end].to_string()), span });
            end
        } else if let Some(punct) = PUNCTS.iter().find(|punct| rest.starts_with(*punct)) {
            tokens.push(Token { tok: Tok::Punct(punct), span });
            punct.len()
        } else {
            return Err(Error::new(span, format!("unexpected character `{}`", c as char)));
        };
        col += rest[..len].chars().filter(|&c| c != '\n').count() as u32;
        line += rest[..len].matches('\n').count() as u32;
        i += len;
    }
    Ok(tokens)
}

/// A statement: its documentation, head tokens, and optional body.
#[derive(Clone, Debug)]
pub struct Node {
    /// `///` lines before the node.
    pub docs: Vec<String>,
    /// Tokens up to the `;` or the body.
    pub head: Vec<Token>,
    /// Child nodes between braces.
    pub body: Option<Vec<Self>>,
    /// Position of the first head token.
    pub span: Span,
}

impl Node {
    /// Returns a cursor over the head.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor { tokens: &self.head, pos: 0, end: self.span }
    }

    /// Returns the children, or none when the node ended with `;`.
    pub fn children(&self) -> &[Self] {
        self.body.as_deref().unwrap_or_default()
    }
}

/// Parses a file into nodes.
pub fn parse(file: usize, src: &str) -> Result<Vec<Node>> {
    let tokens = lex(file, src)?;
    let mut pos = 0;
    let nodes = parse_nodes(&tokens, &mut pos, None)?;
    Ok(nodes)
}

fn parse_nodes(tokens: &[Token], pos: &mut usize, open: Option<Span>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    loop {
        let mut docs = Vec::new();
        while let Some(Token { tok: Tok::Doc(doc), .. }) = tokens.get(*pos) {
            docs.push(doc.clone());
            *pos += 1;
        }
        let Some(first) = tokens.get(*pos) else {
            if let Some(open) = open {
                return Err(Error::new(open, "unclosed `{`"));
            }
            if !docs.is_empty() {
                let span = tokens.last().unwrap().span;
                return Err(Error::new(span, "doc comment documents nothing"));
            }
            return Ok(nodes);
        };
        if first.tok == Tok::Punct("}") {
            if open.is_none() {
                return Err(Error::new(first.span, "unmatched `}`"));
            }
            *pos += 1;
            return Ok(nodes);
        }
        let span = first.span;
        let mut head = Vec::new();
        let mut depth = 0usize;
        let body = loop {
            let Some(token) = tokens.get(*pos) else {
                return Err(Error::new(span, "statement does not end with `;` or `{ ... }`"));
            };
            *pos += 1;
            match token.tok {
                Tok::Punct("(" | "[") => depth += 1,
                Tok::Punct(")" | "]") => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| Error::new(token.span, "unmatched closing delimiter"))?;
                }
                Tok::Punct(";") if depth == 0 => break None,
                Tok::Punct("{") if depth == 0 => {
                    break Some(parse_nodes(tokens, pos, Some(token.span))?);
                }
                _ => {}
            }
            head.push(token.clone());
        };
        nodes.push(Node { docs, head, body, span });
    }
}

/// A position in a node head.
#[derive(Clone)]
pub struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    end: Span,
}

impl<'a> Cursor<'a> {
    /// Returns the next token without consuming it.
    pub fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    /// Returns the span of the next token, or of the end of the head.
    pub fn span(&self) -> Span {
        self.peek().map_or(self.end, |token| token.span)
    }

    /// Returns whether every token was consumed.
    pub fn is_empty(&self) -> bool {
        self.pos == self.tokens.len()
    }

    /// Consumes and returns the next token.
    pub fn bump(&mut self) -> Result<&'a Token> {
        let token = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| Error::new(self.end, "unexpected end of statement"))?;
        self.pos += 1;
        Ok(token)
    }

    /// Consumes `punct` when it is next.
    pub fn eat(&mut self, punct: &str) -> bool {
        if matches!(self.peek(), Some(Token { tok: Tok::Punct(p), .. }) if *p == punct) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Consumes the keyword `word` when it is next.
    pub fn eat_word(&mut self, word: &str) -> bool {
        if self.peek_word() == Some(word) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Returns the next identifier without consuming it.
    pub fn peek_word(&self) -> Option<&'a str> {
        match self.peek() {
            Some(Token { tok: Tok::Ident(name), .. }) => Some(name),
            _ => None,
        }
    }

    /// Requires `punct` next.
    pub fn expect(&mut self, punct: &str) -> Result<()> {
        if self.eat(punct) { Ok(()) } else { Err(self.unexpected(&format!("`{punct}`"))) }
    }

    /// Requires the keyword `word` next.
    pub fn expect_word(&mut self, word: &str) -> Result<()> {
        if self.eat_word(word) { Ok(()) } else { Err(self.unexpected(&format!("`{word}`"))) }
    }

    /// Requires an identifier next.
    pub fn ident(&mut self) -> Result<(&'a str, Span)> {
        match self.peek() {
            Some(Token { tok: Tok::Ident(name), span }) => {
                self.pos += 1;
                Ok((name, *span))
            }
            _ => Err(self.unexpected("an identifier")),
        }
    }

    /// Requires an integer next.
    pub fn int(&mut self) -> Result<u64> {
        match self.peek() {
            Some(Token { tok: Tok::Int(value), .. }) => {
                self.pos += 1;
                Ok(*value)
            }
            _ => Err(self.unexpected("an integer")),
        }
    }

    /// Requires a string next.
    pub fn string(&mut self) -> Result<&'a str> {
        match self.peek() {
            Some(Token { tok: Tok::Str(value), .. }) => {
                self.pos += 1;
                Ok(value)
            }
            _ => Err(self.unexpected("a string")),
        }
    }

    /// Requires the end of the head.
    pub fn finish(&self) -> Result<()> {
        if self.is_empty() { Ok(()) } else { Err(self.unexpected("the end of the statement")) }
    }

    /// Builds an error for an unexpected token where `expected` should be.
    pub fn unexpected(&self, expected: &str) -> Error {
        match self.peek() {
            Some(token) => {
                Error::new(token.span, format!("expected {expected}, found {}", token.tok))
            }
            None => {
                Error::new(self.end, format!("expected {expected}, found the end of the statement"))
            }
        }
    }

    /// Parses `item (, item)*` up to, and excluding, the end of the head or a stop token.
    pub fn comma_list<T>(
        &mut self,
        close: Option<&str>,
        mut item: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let mut items = Vec::new();
        loop {
            let at_end = match close {
                Some(close) => {
                    matches!(self.peek(), Some(Token { tok: Tok::Punct(p), .. }) if *p == close)
                }
                None => self.is_empty(),
            };
            if at_end {
                break;
            }
            items.push(item(self)?);
            if !self.eat(",") {
                break;
            }
        }
        if let Some(close) = close {
            self.expect(close)?;
        }
        Ok(items)
    }
}

/// Converts `snake_case` to `CamelCase`.
pub fn camel(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// Joins doc lines into `#[doc]` attributes at `indent`.
pub fn doc_attrs(docs: &[String], indent: &str) -> String {
    docs.iter().map(|doc| format!("{indent}#[doc = {:?}]\n", format!(" {doc}"))).collect()
}

/// Formats an error with the file name and position.
pub fn render(error: &Error, files: &[(String, String)]) -> String {
    let (name, src) = &files[error.span.file];
    let line = src.lines().nth(error.span.line as usize - 1).unwrap_or_default();
    let caret = " ".repeat(error.span.col as usize - 1);
    format!(
        "{name}:{}:{}: {}\n  {line}\n  {caret}^",
        error.span.line, error.span.col, error.message
    )
}
