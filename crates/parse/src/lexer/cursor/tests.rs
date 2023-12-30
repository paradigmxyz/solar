use super::*;
use expect_test::{expect, Expect};
use std::fmt::Write;

fn check(src: &str, expect: Expect) {
    let mut actual = String::new();
    for token in Cursor::new(src) {
        writeln!(actual, "{token:?}").unwrap();
    }
    expect.assert_eq(&actual);
}

#[test]
fn smoke_test() {
    check(
        "/* my source file */ fn main() { print(\"zebra\"); }\n",
        expect![[r#"
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
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 7 }
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
        expect![[r#"
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
        expect![[r#"
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 4 }
        "#]],
    );
}

#[test]
fn double_str() {
    check(
        r#""a" " " "\n""#,
        expect![[r#"
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 4 }
        "#]],
    );
}

#[test]
fn hex_str() {
    check(
        r#"hex'' hex"ab" h"a" he"a"#,
        expect![[r#"
            RawToken { kind: Literal { kind: HexStr { terminated: true } }, len: 5 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: HexStr { terminated: true } }, len: 7 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: UnknownPrefix, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: UnknownPrefix, len: 2 }
            RawToken { kind: Literal { kind: Str { terminated: false, unicode: false } }, len: 2 }
        "#]],
    );
}

#[test]
fn unicode_str() {
    check(
        r#"unicode'' unicode"ab" u"a" uni"a"#,
        expect![[r#"
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: true } }, len: 9 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: true } }, len: 11 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: UnknownPrefix, len: 1 }
            RawToken { kind: Literal { kind: Str { terminated: true, unicode: false } }, len: 3 }
            RawToken { kind: Whitespace, len: 1 }
            RawToken { kind: UnknownPrefix, len: 3 }
            RawToken { kind: Literal { kind: Str { terminated: false, unicode: false } }, len: 2 }
        "#]],
    );
}
