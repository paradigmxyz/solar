use super::{GlobalState, support::RequestFixture};
use lsp_types::{
    CompletionParams, CompletionResponse, CompletionTextEdit, DidChangeWatchedFilesParams,
    FileChangeType, FileEvent, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use snapbox::{IntoData, str};
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn remappings_change_refreshes_import_completion_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false

        //- /remappings.txt
        pkg/=lib/old/

        //- /src/Main.sol open
        import "pkg/$1";

        //- /lib/old/Old.sol
        contract Old {}

        //- /lib/new/New.sol
        contract New {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");

    assert_eq!(import_completion_labels(&mut state, uri.clone(), position).await, ["pkg/Old.sol"]);

    std::fs::write(fixture.project_path("/remappings.txt"), "pkg/=lib/new/\n").unwrap();
    let remappings_uri = Url::from_file_path(fixture.project_path("/remappings.txt")).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: remappings_uri, typ: FileChangeType::CHANGED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after remappings change should finish")
        .unwrap();

    assert_eq!(import_completion_labels(&mut state, uri, position).await, ["pkg/New.sol"]);
}

async fn import_completion_labels(
    state: &mut GlobalState,
    uri: Url,
    position: Position,
) -> Vec<String> {
    import_completion_items(state, uri, position).await.into_iter().map(|item| item.label).collect()
}

async fn import_completion_items(
    state: &mut GlobalState,
    uri: Url,
    position: Position,
) -> Vec<lsp_types::CompletionItem> {
    let response = crate::handlers::completion(
        state,
        CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    match response {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_import_completion_replaces_the_cursor_suffix() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./De$1p

        //- /src/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");

    let item = import_completion_items(&mut state, uri, position)
        .await
        .into_iter()
        .find(|item| item.label == "./Dependency.sol")
        .unwrap();
    let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("expected a plain text edit")
    };

    assert_eq!(edit.range, lsp_types::Range::new(Position::new(0, 8), Position::new(0, 13)));
    assert_eq!(edit.new_text, "./Dependency.sol");
    assert!(item.additional_text_edits.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn import_completion_does_not_delete_source_after_an_unescaped_newline() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./Dep$1
        contract Victim {}
        ";

        //- /src/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");

    let items = import_completion_items(&mut state, uri, position).await;

    assert!(!items.iter().any(|item| item.label == "./Dependency.sol"));
}

#[tokio::test(flavor = "current_thread")]
async fn import_completion_uses_lsp_lines_after_a_standalone_carriage_return() {
    let mut fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        contract C {}

        //- /src/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );
    fixture.set_open_file_contents("/src/Main.sol", "\rcontract C {}\nimport \"./Dep");
    let mut state = fixture.state();
    let uri = Url::from_file_path(fixture.project_path("/src/Main.sol")).unwrap();

    let contract_items =
        import_completion_items(&mut state, uri.clone(), Position::new(1, 13)).await;
    assert!(!contract_items.iter().any(|item| item.label == "./Dependency.sol"));

    let item = import_completion_items(&mut state, uri, Position::new(2, 13))
        .await
        .into_iter()
        .find(|item| item.label == "./Dependency.sol")
        .unwrap();
    let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("expected a plain text edit")
    };
    assert_eq!(edit.range, lsp_types::Range::new(Position::new(2, 8), Position::new(2, 13)));
}

#[test]
fn completes_relative_import_files_and_directories() {
    let mut fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./$1";

        //- /src/Local.sol
        contract Local {}

        //- /src/nested/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );
    fixture.set_open_file_contents("/src/Unsaved.sol", "contract Unsaved {}");

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Local.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:10
insert_text_format=<none>
new_text:
./Local.sol

label=./Main.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:10
insert_text_format=<none>
new_text:
./Main.sol

label=./Unsaved.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:10
insert_text_format=<none>
new_text:
./Unsaved.sol

label=./nested/
kind=Folder
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:10
insert_text_format=<none>
new_text:
./nested/

"#]],
    );
}

#[test]
fn completes_single_quoted_multiline_imports() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import {
            Target
        } from './T$1';

        //- /src/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 2:8-2:11
insert_text_format=<none>
new_text:
./Target.sol

"#]],
    );
}

#[test]
fn completes_remapped_imports_from_the_deepest_foundry_workspace() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/"]

        //- /lib/outer/Outer.sol
        contract Outer {}

        //- /packages/app/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/"]

        //- /packages/app/src/Main.sol open
        import "pkg/$1";

        //- /packages/app/lib/inner/Inner.sol
        contract Inner {}
        "#,
        "/packages/app/src/Main.sol",
    );

    fixture.check_completion_details_with_trigger(
        "$1",
        "/",
        str![[r#"
label=pkg/Inner.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:12
insert_text_format=<none>
new_text:
pkg/Inner.sol

"#]],
    );
}

#[test]
fn completes_partial_configured_remapping_prefixes() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = [
            "foo/=lib/global/",
            "src:@openzeppelin/=lib/openzeppelin-contracts/contracts/",
            "src:Alias=lib/Alias.sol",
            "test:@openzeppelin-test/=lib/openzeppelin-test/contracts/",
            "test:foo/bar/=lib/foo-bar/",
        ]

        //- /src/Main.sol open
        import "@o$1";
        import "A$2";
        import "foo/b$3";

        //- /lib/Alias.sol
        contract Alias {}

        //- /lib/global/Other.sol
        contract Other {}

        //- /lib/openzeppelin-contracts/contracts/Token.sol
        contract Token {}

        //- /lib/openzeppelin-test/contracts/TestToken.sol
        contract TestToken {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=@openzeppelin/
kind=Folder
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:10
insert_text_format=<none>
new_text:
@openzeppelin/

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=Alias
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 1:8-1:9
insert_text_format=<none>
new_text:
Alias

label=Alias.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 1:8-1:9
insert_text_format=<none>
new_text:
Alias.sol

"#]],
    );
    fixture.check_completion_details("$3", str![""]);
}

#[test]
fn completes_package_imports_from_configured_include_paths() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        libs = ["node_modules"]

        //- /src/Main.sol open
        import "@scope/package/$1";

        //- /node_modules/@scope/package/Token.sol
        contract Token {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=@scope/package/Token.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:23
insert_text_format=<none>
new_text:
@scope/package/Token.sol

"#]],
    );
}

#[test]
fn completes_valid_escaped_import_prefixes() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import ".\x2fTar$1";

        //- /src/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:16
insert_text_format=<none>
new_text:
./Target.sol

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn escaped_import_prefixes_set_complete_client_filter_text() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import ".\x2fTar$1";

        //- /src/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");

    let items = import_completion_items(&mut state, uri, position).await;

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "./Target.sol");
    assert_eq!(items[0].filter_text.as_deref(), Some(".\\x2fTarget.sol"));
}

#[tokio::test(flavor = "current_thread")]
async fn import_filter_text_remains_matchable_as_the_prefix_grows() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./$1";
        import "./Tar$2";

        //- /src/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();

    for marker in ["$1", "$2"] {
        let (uri, position) = fixture.marker_location(marker);
        let items = import_completion_items(&mut state, uri, position).await;
        let target = items.iter().find(|item| item.label == "./Target.sol").unwrap();
        assert_eq!(target.filter_text.as_deref(), Some("./Target.sol"));
    }
}

#[test]
fn completes_imports_across_escaped_line_continuations() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./nested/$2\
        Tar$1get.sol";

        //- /src/nested/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./nested/Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 1:0-1:10
additional_text_edit=0:8-1:0 new_text=""
insert_text_format=<none>
new_text:
./nested/Target.sol

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=./nested/Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:18
additional_text_edit=0:18-1:10 new_text=""
insert_text_format=<none>
new_text:
./nested/Target.sol

"#]],
    );
}

#[test]
fn opening_quote_completion_does_not_return_an_out_of_range_edit() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import $1"./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details("$1", str![""]);
}

#[test]
fn unterminated_import_completion_does_not_replace_the_next_line() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./Dep$1
        contract Main { string value = "ordinary"; }

        //- /src/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Dependency.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:13
insert_text_format=<none>
new_text:
./Dependency.sol

"#]],
    );
}

#[test]
fn completes_extensionless_disk_and_overlay_import_candidates() {
    let mut fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./Dep$1";

        //- /src/Dependency
        contract Dependency {}

        //- /src/Dependency.md
        not Solidity
        "#,
        "/src/Main.sol",
    );
    fixture.set_open_file_contents("/src/DependencyOverlay", "contract DependencyOverlay {}");

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Dependency
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:13
insert_text_format=<none>
new_text:
./Dependency

label=./DependencyOverlay
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:13
insert_text_format=<none>
new_text:
./DependencyOverlay

"#]],
    );
}

#[test]
fn completes_an_extensionless_remapping_target_as_a_file() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["src:Alias=lib/Alias"]

        //- /src/Main.sol open
        import "A$1";

        //- /lib/Alias
        contract Alias {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=Alias
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:9
insert_text_format=<none>
new_text:
Alias

"#]],
    );
}

#[test]
fn incomplete_escape_in_an_import_does_not_fall_back_to_symbol_completion() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import ".\x$1";
        contract Main {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details("$1", str![""]);
}

#[test]
fn quote_trigger_completes_imports_but_not_ordinary_strings() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /Main.sol open
        import "$1";
        import '$3';
        contract Main {
            string value = "$2";
            string singleQuoted = '$4';
        }

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );

    fixture.check_completion_details_with_trigger(
        "$1",
        "\"",
        str![[r#"
label=Main.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:8
insert_text_format=<none>
new_text:
Main.sol

label=Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:8
insert_text_format=<none>
new_text:
Target.sol

"#]],
    );
    fixture.check_completion_details_with_trigger("$2", "\"", str![""]);
    fixture.check_completion_details_with_trigger(
        "$3",
        "'",
        str![[r#"
label=Main.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 1:8-1:8
insert_text_format=<none>
new_text:
Main.sol

label=Target.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 1:8-1:8
insert_text_format=<none>
new_text:
Target.sol

"#]],
    );
    fixture.check_completion_details_with_trigger("$4", "'", str![""]);
}

#[test]
fn unowned_import_completion_does_not_fall_back_to_symbols() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /owned/foundry.toml

        //- /unowned/Main.sol open
        import "./$1";
        contract Main {}
        "#,
        "/unowned/Main.sol",
    );

    fixture.check_completion_details("$1", str![""]);
}

#[test]
fn import_completion_text_edits_use_utf16_ranges_and_escape_non_ascii_paths() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        /* 😀 */ import "./Token$1";

        //- /src/Token😀.sol
        contract Token {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Token😀.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:17-0:24
insert_text_format=<none>
new_text:
./Token\xF0\x9F\x98\x80.sol

"#]]
        .raw(),
    );
}

#[test]
fn import_completion_escapes_the_active_string_delimiter() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import './Owner$1';

        //- /src/Owner'sToken.sol
        contract Token {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=./Owner'sToken.sol
kind=File
detail=<none>
sort_text=<none>
text_edit=edit 0:8-0:15
insert_text_format=<none>
new_text:
./Owner\'sToken.sol

"#]]
        .raw(),
    );
}
