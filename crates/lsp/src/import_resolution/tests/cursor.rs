use super::super::ImportCursor;

fn marked_source(source: &str) -> (String, usize) {
    let cursor = source.find("$0").unwrap();
    (source.replacen("$0", "", 1), cursor)
}

#[test]
fn import_cursor_recovers_an_unterminated_path_without_consuming_the_next_line() {
    let (source, offset) = marked_source("import \"./Dep$0\ncontract C {}\n");

    let cursor = ImportCursor::at(&source, offset).unwrap();

    assert_eq!(cursor.path_prefix, "./Dep");
    assert_eq!(&source[cursor.replacement_range()], "./Dep");
    assert_eq!(cursor.complete_path(), None);
}

#[test]
fn import_cursor_does_not_use_a_quote_on_the_next_line_as_the_terminator() {
    let (source, offset) =
        marked_source("import \"./Dep$0\ncontract C { string value = \"ordinary\"; }\n");

    let cursor = ImportCursor::at(&source, offset).unwrap();

    assert_eq!(cursor.path_prefix, "./Dep");
    assert_eq!(&source[cursor.replacement_range()], "./Dep");
    assert_eq!(cursor.complete_path(), None);
}

#[test]
fn import_cursor_treats_backslash_bare_cr_as_an_unescaped_line_break() {
    let (source, offset) =
        marked_source("import \"./Dep$0\\\rcontract C { string value = \"ordinary\"; }\r");

    let cursor = ImportCursor::at(&source, offset).unwrap();

    assert_eq!(cursor.path_prefix, "./Dep");
    assert_eq!(&source[cursor.replacement_range()], "./Dep");
    assert_eq!(cursor.complete_path(), None);
}

#[test]
fn import_cursor_does_not_replace_unknown_unterminated_suffixes() {
    let (source, offset) = marked_source("import \"./Dep$0 contract C {}");

    let cursor = ImportCursor::at(&source, offset).unwrap();

    assert_eq!(cursor.path_prefix, "./Dep");
    assert_eq!(&source[cursor.replacement_range()], "./Dep");
    assert_eq!(cursor.complete_path(), None);
}

#[test]
fn import_cursor_supports_escaped_line_continuations() {
    for newline in ["\n", "\r\n"] {
        let marked = format!("import \"./nested/\\{newline}    Tar$0get.sol\";");
        let (source, offset) = marked_source(&marked);

        let cursor = ImportCursor::at(&source, offset).unwrap();

        assert_eq!(cursor.decoded_path_prefix().as_deref(), Some("./nested/Tar"));
        assert_eq!(
            &source[cursor.replacement_range()],
            format!("./nested/\\{newline}    Target.sol")
        );
        assert_eq!(
            cursor.complete_path(),
            Some(format!("./nested/\\{newline}    Target.sol").as_str())
        );
    }
}

#[test]
fn import_cursor_recovers_after_an_import_missing_its_semicolon() {
    let (source, offset) = marked_source("import \"./First.sol\"\nimport \"./Second$0.sol\";");

    let cursor = ImportCursor::at(&source, offset).unwrap();

    assert_eq!(cursor.path_prefix, "./Second");
    assert_eq!(&source[cursor.replacement_range()], "./Second.sol");
    assert_eq!(cursor.complete_path(), Some("./Second.sol"));
}

#[test]
fn import_cursor_recognizes_import_forms_and_rejects_ordinary_strings() {
    for marked in [
        "import \"./Dep$0\";",
        "import \"./Dep$0\" as Dependency;",
        "import * as Dependency from \"./Dep$0\";",
        "import {A, B as C} from \"./Dep$0\";",
        "import {\n    A\n} from './Dep$0';",
    ] {
        let (source, offset) = marked_source(marked);
        let cursor = ImportCursor::at(&source, offset)
            .unwrap_or_else(|| panic!("expected an import cursor for {source:?}"));

        assert_eq!(cursor.path_prefix, "./Dep");
        assert_eq!(&source[cursor.replacement_range()], "./Dep");
        assert_eq!(cursor.complete_path(), Some("./Dep"));
    }

    for marked in [
        "string constant VALUE = \"./Dep$0\";",
        "contract C { function f() external { string memory x = \"./Dep$0\"; } }",
    ] {
        let (source, offset) = marked_source(marked);
        assert_eq!(ImportCursor::at(&source, offset), None, "source: {source:?}");
    }
}
