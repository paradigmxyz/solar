use super::*;
use snapbox::{IntoData, str};
use std::fmt::Write;

fn check(src: &str, data: impl IntoData) {
    let mut actual = String::new();
    for token in Cursor::new(src) {
        writeln!(actual, "{token:?}").unwrap();
    }
    snapbox::assert_data_eq!(actual.trim(), data);
}

#[test]
fn smoke_test() {
    check(
        "/* my source file */ fn main() { print(\"zebra\"); }\n",
        str![[r#"
RawToken { kind: BlockComment { is_doc: false, terminated: true }, len: 20 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 2 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 4 }
RawToken { kind: OpenParen, len: 1 }
RawToken { kind: CloseParen, len: 1 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: OpenBrace, len: 1 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 5 }
RawToken { kind: OpenParen, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 7 }
RawToken { kind: CloseParen, len: 1 }
RawToken { kind: Semi, len: 1 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: CloseBrace, len: 1 }
RawToken { kind: Whitespace, len: 1 }
"#]],
    );
}

#[test]
fn comment_flavors() {
    check(
        r"
// line
//// line as well
/// doc line
/* block */
/**/
/*** also block */
/** doc block */
",
        str![[r#"
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: LineComment { is_doc: false }, len: 7 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: LineComment { is_doc: false }, len: 17 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: LineComment { is_doc: true }, len: 12 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: false, terminated: true }, len: 11 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: false, terminated: true }, len: 4 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: false, terminated: true }, len: 18 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: true, terminated: true }, len: 16 }
RawToken { kind: Whitespace, len: 1 }
"#]],
    )
}

#[test]
fn single_str() {
    check(
        "'a' ' ' '\\n'",
        str![[r#"
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 4 }
"#]],
    );
}

#[test]
fn double_str() {
    check(
        r#""a" " " "\n""#,
        str![[r#"
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 4 }
"#]],
    );
}

#[test]
fn hex_str() {
    check(
        r#"hex'' hex"ab" h"a" he"a"#,
        str![[r#"
RawToken { kind: Literal { kind: Str { kind: Hex, terminated: true } }, len: 5 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Hex, terminated: true } }, len: 7 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 2 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: false } }, len: 2 }
"#]],
    );
}

#[test]
fn unicode_str() {
    check(
        r#"unicode'' unicode"ab" u"a" uni"a"#,
        str![[r#"
RawToken { kind: Literal { kind: Str { kind: Unicode, terminated: true } }, len: 9 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Unicode, terminated: true } }, len: 11 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 3 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 3 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: false } }, len: 2 }
"#]],
    );
}

#[test]
fn random_unicode() {
    check(
        r#"
è

"è"
hex"è"
unicode"è"

'è'
hex'è'
unicode'è'

hex👀
unicode👀

//è
/*è */

///è
/**è */

.è
0.è
1.eè
1.e1è
"#,
        str![[r#"
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Unknown, len: 2 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 4 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Hex, terminated: true } }, len: 7 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Unicode, terminated: true } }, len: 11 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: Literal { kind: Str { kind: Str, terminated: true } }, len: 4 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Hex, terminated: true } }, len: 7 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Str { kind: Unicode, terminated: true } }, len: 11 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: Ident, len: 3 }
RawToken { kind: Unknown, len: 4 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Ident, len: 7 }
RawToken { kind: Unknown, len: 4 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: LineComment { is_doc: false }, len: 4 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: false, terminated: true }, len: 7 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: LineComment { is_doc: true }, len: 5 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: BlockComment { is_doc: true, terminated: true }, len: 8 }
RawToken { kind: Whitespace, len: 2 }
RawToken { kind: Dot, len: 1 }
RawToken { kind: Unknown, len: 2 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Rational { base: Decimal, empty_exponent: false } }, len: 2 }
RawToken { kind: Unknown, len: 2 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Int { base: Decimal, empty_int: false } }, len: 1 }
RawToken { kind: Dot, len: 1 }
RawToken { kind: Ident, len: 1 }
RawToken { kind: Unknown, len: 2 }
RawToken { kind: Whitespace, len: 1 }
RawToken { kind: Literal { kind: Int { base: Decimal, empty_int: false } }, len: 1 }
RawToken { kind: Dot, len: 1 }
RawToken { kind: Ident, len: 2 }
RawToken { kind: Unknown, len: 2 }
RawToken { kind: Whitespace, len: 1 }
"#]],
    );
}

#[test]
fn windows_line_ending() {
    check(
        "/// doc line\r\n",
        str![[r#"
RawToken { kind: LineComment { is_doc: true }, len: 12 }
RawToken { kind: Whitespace, len: 2 }
"#]],
    );
}
