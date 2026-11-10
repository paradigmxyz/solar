//! SMT-LIB S-expressions: the commands of a script and the responses a solver prints.
//!
//! [`parse`] reads the top-level expressions of a complete text, and [`Reader`] frames them
//! from output that arrives in pieces, such as a solver's stdout read line by line. Both follow
//! SMT-LIB's lexical rules, so parentheses inside comments, strings and quoted symbols do not
//! nest: `;` starts a comment that runs to the end of the line, a string literal escapes `"` as
//! `""`, and a `|`-quoted symbol holds any character but `|`. Z3 escapes `"` in the messages it
//! prints as `\"`, so a backslash in a string escapes the character after it.
//!
//! Atoms keep their text as written, quotes included, and [`Sexp`]'s `Display` prints an
//! expression on one line with single spaces, so a parsed command prints back as an equivalent
//! command.

use std::fmt;

/// An S-expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sexp {
    /// A symbol, keyword, numeral, bitvector or string literal, as written.
    Atom(String),
    List(Vec<Self>),
}

impl Sexp {
    /// The atom's text.
    pub fn as_atom(&self) -> Option<&str> {
        match self {
            Self::Atom(atom) => Some(atom),
            Self::List(_) => None,
        }
    }

    /// The list's elements.
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::Atom(_) => None,
            Self::List(items) => Some(items),
        }
    }

    /// The atom at the head of a list, such as a command's name.
    pub fn head(&self) -> Option<&str> {
        self.as_list()?.first()?.as_atom()
    }
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Atom(atom) => f.write_str(atom),
            Self::List(items) => {
                f.write_str("(")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// Text that is not a sequence of S-expressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A `)` with no `(` to close, at this byte offset.
    Unopened(usize),
    /// The text ends inside a list, string or quoted symbol.
    Unterminated,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unopened(offset) => write!(f, "unmatched `)` at byte {offset}"),
            Self::Unterminated => f.write_str("unterminated S-expression"),
        }
    }
}

impl std::error::Error for Error {}

/// The top-level S-expressions of `text`, in order, ending after the first error.
pub fn parse(text: &str) -> Parser<'_> {
    Parser { text, offset: 0 }
}

/// An iterator over the top-level S-expressions of a text; see [`parse`].
#[derive(Clone, Debug)]
pub struct Parser<'a> {
    text: &'a str,
    offset: usize,
}

impl Iterator for Parser<'_> {
    type Item = Result<Sexp, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match next(self.text, self.offset, true) {
            Ok(Some((expression, end))) => {
                self.offset = end;
                Some(Ok(expression))
            }
            Ok(None) => None,
            Err(error) => {
                self.offset = self.text.len();
                Some(Err(error))
            }
        }
    }
}

/// Frames top-level S-expressions from text that arrives in pieces.
#[derive(Clone, Debug, Default)]
pub struct Reader {
    buffer: String,
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends text, such as a line of output with its newline.
    pub fn push(&mut self, text: &str) {
        self.buffer.push_str(text);
    }

    /// The next complete expression, or `None` until more text arrives. An atom is complete once
    /// a delimiter, such as a newline, follows it. After an unmatched `)`, the reader goes on with
    /// the text after it.
    pub fn read(&mut self) -> Option<Result<Sexp, Error>> {
        match next(&self.buffer, 0, false) {
            Ok(Some((expression, end))) => {
                self.buffer.drain(..end);
                Some(Ok(expression))
            }
            Ok(None) => None,
            Err(error) => {
                // Drop the text up to the error, so the reader can go on. Without the end of
                // the text, an expression is incomplete rather than unterminated.
                if let Error::Unopened(offset) = error {
                    self.buffer.drain(..=offset);
                }
                Some(Err(error))
            }
        }
    }
}

/// The expression that starts at or after byte `offset` of `text`, with the offset where it ends;
/// `None` when only whitespace and comments remain, or, unless `eof`, when the expression may go
/// on in text that has not arrived.
fn next(text: &str, offset: usize, eof: bool) -> Result<Option<(Sexp, usize)>, Error> {
    let bytes = text.as_bytes();
    let mut stack: Vec<Vec<Sexp>> = Vec::new();
    let mut position = offset;
    loop {
        let Some(&byte) = bytes.get(position) else {
            return if stack.is_empty() {
                Ok(None)
            } else if eof {
                Err(Error::Unterminated)
            } else {
                Ok(None)
            };
        };
        let atom = match byte {
            b' ' | b'\t' | b'\n' | b'\r' => {
                position += 1;
                continue;
            }
            b';' => {
                position = bytes[position..]
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .map_or(bytes.len(), |length| position + length);
                continue;
            }
            b'(' => {
                stack.push(Vec::new());
                position += 1;
                continue;
            }
            b')' => {
                let Some(items) = stack.pop() else { return Err(Error::Unopened(position)) };
                position += 1;
                Sexp::List(items)
            }
            b'"' | b'|' => {
                let Some(end) = quoted_end(bytes, position) else {
                    return if eof { Err(Error::Unterminated) } else { Ok(None) };
                };
                let atom = Sexp::Atom(text[position..end].to_owned());
                position = end;
                atom
            }
            _ => {
                let start = position;
                while bytes.get(position).is_some_and(|&byte| !is_delimiter(byte)) {
                    position += 1;
                }
                if position == bytes.len() && !eof && stack.is_empty() {
                    return Ok(None);
                }
                Sexp::Atom(text[start..position].to_owned())
            }
        };
        match stack.last_mut() {
            Some(items) => items.push(atom),
            None => return Ok(Some((atom, position))),
        }
    }
}

/// The offset just past the string or quoted symbol that starts at `start`.
fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = bytes[start];
    let mut position = start + 1;
    while let Some(&byte) = bytes.get(position) {
        if quote == b'"' && byte == b'\\' {
            position += 2;
        } else if byte == quote {
            // `""` is an escaped quote in a string.
            if quote == b'"' && bytes.get(position + 1) == Some(&b'"') {
                position += 2;
            } else {
                return Some(position + 1);
            }
        } else {
            position += 1;
        }
    }
    None
}

fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b';' | b'"' | b'|')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(text: &str) -> Result<Vec<Sexp>, Error> {
        parse(text).collect()
    }

    fn display(text: &str) -> Vec<String> {
        all(text).unwrap().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn multiline_get_value() {
        let output = "sat\n((z #x00)\n (w #x01))\n(:reason-unknown \"\")\n";
        assert_eq!(display(output), ["sat", "((z #x00) (w #x01))", "(:reason-unknown \"\")"]);
        let values = &all(output).unwrap()[1];
        let pairs: Vec<_> = values.as_list().unwrap().iter().map(ToString::to_string).collect();
        assert_eq!(pairs, ["(z #x00)", "(w #x01)"]);
    }

    #[test]
    fn delimiters_inside_atoms() {
        // A Z3 error that spans lines, with parentheses and an escaped quote in its message.
        let error =
            "(error \"line 1: unknown constant x\\\"y\nLegal (bool) (default: true)\")\nunsat\n";
        let expressions = all(error).unwrap();
        assert_eq!(expressions.len(), 2);
        assert_eq!(expressions[0].head(), Some("error"));
        assert_eq!(expressions[1].as_atom(), Some("unsat"));
        assert_eq!(display("(echo \"a\"\")\"\"(b\")"), ["(echo \"a\"\")\"\"(b\")"]);
        assert_eq!(display("(declare-const |x) (y| Int)"), ["(declare-const |x) (y| Int)"]);
        assert_eq!(
            display("; a comment (\n(assert (> x 1)) ; ) another\n(check-sat)"),
            ["(assert (> x 1))", "(check-sat)"]
        );
    }

    #[test]
    fn errors() {
        assert_eq!(all("(a))"), Err(Error::Unopened(3)));
        assert_eq!(all("(a (b)"), Err(Error::Unterminated));
        assert_eq!(all("\"open"), Err(Error::Unterminated));
        assert_eq!(all(" ; only a comment\n"), Ok(Vec::new()));
        // The expressions before an error still come out, and nothing after it.
        let mut killed = parse("sat\n((x #x01)\n (y");
        assert_eq!(killed.next(), Some(Ok(Sexp::Atom("sat".into()))));
        assert_eq!(killed.next(), Some(Err(Error::Unterminated)));
        assert_eq!(killed.next(), None);
    }

    #[test]
    fn reader_frames_lines() {
        let mut reader = Reader::new();
        reader.push("((z #x00)\n");
        assert_eq!(reader.read(), None);
        reader.push(" (w \"a)\nb\"))\nsa");
        assert_eq!(reader.read().unwrap().unwrap().to_string(), "((z #x00) (w \"a)\nb\"))");
        // An atom at the end of the text may go on.
        assert_eq!(reader.read(), None);
        reader.push("t\n");
        assert_eq!(reader.read(), Some(Ok(Sexp::Atom("sat".into()))));
        assert_eq!(reader.read(), None);
        reader.push(")\nunsat\n");
        assert_eq!(reader.read(), Some(Err(Error::Unopened(1))));
        assert_eq!(reader.read(), Some(Ok(Sexp::Atom("unsat".into()))));
        assert_eq!(reader.read(), None);
    }
}
