use super::*;

/*
#[test]
fn test_unescape_str_warn() {
    fn check(literal: &str, expected: &[(Range<usize>, Result<char, EscapeError>)]) {
        let mut unescaped = Vec::with_capacity(literal.len());
        unescape_literal(literal, Mode::Str, &mut |range, res| unescaped.push((range, res)));
        assert_eq!(unescaped, expected);
    }

    // Check we can handle escaped newlines at the end of a file.
    check("\\\n", &[]);
    check("\\\n ", &[]);

    check(
        "\\\n \u{a0} x",
        &[
            (0..5, Err(EscapeError::UnskippedWhitespaceWarning)),
            (3..5, Ok('\u{a0}')),
            (5..6, Ok(' ')),
            (6..7, Ok('x')),
        ],
    );
    check("\\\n  \n  x", &[(0..7, Err(EscapeError::MultipleSkippedLinesWarning)), (7..8, Ok('x'))]);
}
*/

#[test]
fn test_unescape_str_good() {
    fn check(literal_text: &str, expected: &str) {
        let mut buf = Ok(String::with_capacity(literal_text.len()));
        unescape_literal(literal_text, '"', Mode::Str, &mut |range, c| {
            if let Ok(b) = &mut buf {
                match c {
                    Ok(c) => b.push(c),
                    Err(e) => buf = Err((range, e)),
                }
            }
        });
        assert_eq!(buf.as_deref(), Ok(expected))
    }

    check("foo", "foo");
    check("", "");
    check(" \t", " \t");

    // check("hello \\\n     world", "hello world");
    check("thread's", "thread's")
}

#[test]
fn test_unescape_hex_str_good() {
    fn check(literal_text: &str, expected: &str) {
        let mut buf = Ok(String::with_capacity(literal_text.len()));
        unescape_literal(literal_text, '"', Mode::HexStr, &mut |range, c| {
            if let Ok(b) = &mut buf {
                match c {
                    Ok(c) => b.push(c),
                    Err(e) => buf = Err((range, e)),
                }
            }
        });
        assert_eq!(buf.as_deref(), Ok(expected))
    }

    check("", "");
    check("11", "11");
    check("1111", "1111");
    check("1_1_1_1", "1111");
}
