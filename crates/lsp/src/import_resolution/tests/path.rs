use super::super::{import_path_at, import_path_at_for_completion};

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
fn completion_matches_parser_import_ranges_at_every_boundary() {
    for source in [
        r#"import "./Dep.sol"; contract C { string s = "ordinary"; }"#,
        "import {Dep as Alias} from './Dep.sol'; // import './Fake.sol';",
        r#"/* import "./Fake.sol"; */ import "./Dep.sol" as Dep;"#,
        r#"import * as Dep from "./😀.sol";"#,
        r#"contract C { string s = unicode"./Dep.sol"; bytes s2 = hex"abcd"; }"#,
    ] {
        for cursor in (0..=source.len()).filter(|&cursor| source.is_char_boundary(cursor)) {
            assert_eq!(
                import_path_at_for_completion(source, cursor),
                import_path_at(source, cursor),
                "cursor {cursor} in {source}",
            );
        }
    }
}
