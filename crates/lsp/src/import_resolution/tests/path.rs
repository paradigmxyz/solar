use super::super::{import_path_at, import_path_at_for_completion, parse_import_path};

#[test]
fn completion_recovers_only_unterminated_import_paths() {
    let open_before_string = "import \"./Dep\ncontract Main { string value = \"ordinary\"; }";
    let named = "import { Dependency } from './Dep";
    let ordinary = "contract Main { string value = \"./Dep";
    let past_line_break = open_before_string.find("contract").unwrap() + "contract".len();

    assert!(import_path_at(open_before_string, open_before_string.find('\n').unwrap()).is_none());
    for (source, cursor, expected) in [
        (open_before_string, open_before_string.find('\n').unwrap(), Some((8..13, b'"'))),
        (named, named.len(), Some((28..33, b'\''))),
        (ordinary, ordinary.len(), None),
        (open_before_string, past_line_break, None),
    ] {
        let import = import_path_at_for_completion(source, cursor);
        assert_eq!(
            import.map(|import| {
                assert_eq!(import.raw_path, "./Dep");
                (import.content_range, import.delimiter)
            }),
            expected,
            "cursor {cursor} in {source}"
        );
    }
}

#[test]
fn import_paths_match_parser_at_every_boundary() {
    for (source, completion) in [
        (r#"import "./Dep.sol"; contract C { string s = "ordinary"; }"#, true),
        ("import {Dep as Alias} from './Dep.sol'; // import './Fake.sol';", true),
        (r#"/* import "./Fake.sol"; */ import "./Dep.sol" as Dep;"#, true),
        (r#"import * as Dep from "./😀.sol";"#, true),
        (r#"contract C { string s = unicode"./Dep.sol"; bytes s2 = hex"abcd"; }"#, true),
        (r#"import "./Dep" "suffix";"#, false),
        (r#"import "./Dep"#, false),
    ] {
        for cursor in (0..=source.len()).filter(|&cursor| source.is_char_boundary(cursor)) {
            let expected = parse_import_path(source, cursor);
            assert_eq!(import_path_at(source, cursor), expected, "cursor {cursor} in {source}");
            if completion {
                assert_eq!(
                    import_path_at_for_completion(source, cursor),
                    expected,
                    "cursor {cursor} in {source}",
                );
            }
        }
    }
}
