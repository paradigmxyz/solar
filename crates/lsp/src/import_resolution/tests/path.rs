use super::super::{
    import_path_at, import_path_at_for_completion, may_complete_import_string, may_complete_string,
    parse_import_path,
};
use crop::Rope;

#[test]
fn import_string_guard_handles_quotes_and_line_breaks_across_chunks() {
    for (prefix, cursor_byte, expected) in [
        ("", None, false),
        ("", Some(b'"'), true),
        ("import ", Some(b'\''), true),
        ("contract C { value", None, false),
        ("\"earlier\"\ncontract C", None, false),
        ("\"earlier\"\r\ncontract C", None, false),
        ("\"earlier\"\rcontract C", None, false),
        ("import \"./", None, true),
        ("import './", None, true),
        ("/* 😀 */ import \"./", None, true),
        ("import \"./\\\nDep", None, true),
        ("import \"./\\\r\nDep", None, true),
        ("import \"./\\\rDep", None, true),
        ("import \"./\\\nDep\nordinary", None, false),
        // Backslash parity and lexical context remain the parser's responsibility.
        ("import \"./\\\\\nDep", None, true),
    ] {
        for first in (0..=prefix.len()).filter(|&offset| prefix.is_char_boundary(offset)) {
            for second in (first..=prefix.len()).filter(|&offset| prefix.is_char_boundary(offset)) {
                let chunks = [&prefix[..first], &prefix[first..second], &prefix[second..]];
                assert_eq!(
                    may_complete_string(chunks.into_iter(), cursor_byte),
                    expected,
                    "prefix {prefix:?}, splits {first}/{second}, cursor byte {cursor_byte:?}",
                );
            }
        }

        let mut source = format!("// {}\n{prefix}", "padding".repeat(1024));
        let cursor = source.len();
        if let Some(byte) = cursor_byte {
            source.push(char::from(byte));
        }
        let rope = Rope::from(source.as_str());
        assert!(rope.chunks().count() > 1);
        assert_eq!(may_complete_import_string(&rope, cursor), expected, "prefix {prefix:?}");
    }
}

#[test]
fn import_string_guard_rejects_invalid_byte_cursors() {
    let source = Rope::from("import \"./😀");
    assert!(!may_complete_import_string(&source, source.byte_len() + 1));
    assert!(!may_complete_import_string(&source, source.byte_len() - 1));
}

#[test]
fn completion_recovers_an_unterminated_import_before_an_unrelated_string() {
    let source = "import \"./Dep\ncontract Main { string value = \"ordinary\"; }";
    let cursor = source.find('\n').unwrap();

    assert!(import_path_at(source, cursor).is_none());
    let import = import_path_at_for_completion(source, cursor).unwrap();

    assert_eq!(import.raw_path, "./Dep");
    assert_eq!(import.content_range, 8..13);
    assert_eq!(import.delimiter, b'"');
}

#[test]
fn completion_recovers_a_single_quoted_named_import() {
    let source = "import { Dependency } from './Dep";
    let cursor = source.len();
    let import = import_path_at_for_completion(source, cursor).unwrap();

    assert_eq!(import.raw_path, "./Dep");
    assert_eq!(&source[import.content_range], "./Dep");
    assert_eq!(import.delimiter, b'\'');
}

#[test]
fn completion_does_not_recover_an_unterminated_ordinary_string() {
    let source = "contract Main { string value = \"./Dep";

    assert!(import_path_at_for_completion(source, source.len()).is_none());
}

#[test]
fn completion_does_not_recover_past_an_unescaped_line_break() {
    let source = "import \"./Dep\ncontract Main { string value = \"ordinary\"; }";
    let cursor = source.find("contract").unwrap() + "contract".len();

    assert!(import_path_at_for_completion(source, cursor).is_none());
}

#[test]
fn valid_import_paths_match_parser_at_every_boundary() {
    for source in [
        r#"import "./Dep.sol"; contract C { string s = "ordinary"; }"#,
        "import {Dep as Alias} from './Dep.sol'; // import './Fake.sol';",
        r#"/* import "./Fake.sol"; */ import "./Dep.sol" as Dep;"#,
        r#"import * as Dep from "./😀.sol";"#,
        r#"contract C { string s = unicode"./Dep.sol"; bytes s2 = hex"abcd"; }"#,
    ] {
        for cursor in (0..=source.len()).filter(|&cursor| source.is_char_boundary(cursor)) {
            let expected = parse_import_path(source, cursor);
            assert_eq!(import_path_at(source, cursor), expected, "cursor {cursor} in {source}");
            assert_eq!(
                import_path_at_for_completion(source, cursor),
                expected,
                "cursor {cursor} in {source}",
            );
        }
    }
}

#[test]
fn definition_matches_parser_import_ranges_at_every_boundary() {
    for source in [r#"import "./Dep" "suffix";"#, r#"import "./Dep"#] {
        for cursor in (0..=source.len()).filter(|&cursor| source.is_char_boundary(cursor)) {
            assert_eq!(
                import_path_at(source, cursor),
                parse_import_path(source, cursor),
                "cursor {cursor} in {source}"
            );
        }
    }
}
