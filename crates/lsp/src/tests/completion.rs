use super::{GlobalState, support::RequestFixture};
use lsp_types::{
    CompletionParams, CompletionResponse, DidChangeWatchedFilesParams, FileChangeType, FileEvent,
    PartialResultParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
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
text_edit=edit 1:0-1:14
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
additional_text_edit=0:18-1:14 new_text=""
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
./Owner\x27sToken.sol

"#]]
        .raw(),
    );
}

#[test]
fn uses_utf16_ranges_with_non_bmp_source_text() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        // 😀
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 1:0-1:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_contracts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_named_function_parameters_and_return() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 amount, uint256) external pure returns (uint256 total) {
                return amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_unnamed_function_parameter_and_return() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256, address recipient) external pure returns (uint256) {
                return uint160(recipient);
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param recipient $2
    /// @return $3$0

"#]],
    );
}

#[test]
fn deduplicates_parameter_names_and_keeps_all_returns() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 amount, uint256 amount)
                external
                pure
                returns (uint256 total, uint256)
            {
                return (amount, amount);
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3
    /// @return $4$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_contract_kinds() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        abstract contract AbstractVault {}
        ///$2
        interface IVault {}
        ///$3
        library VaultMath {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec abstract contract documentation
kind=Snippet
detail=abstract contract AbstractVault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec interface documentation
kind=Snippet
detail=interface IVault
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec library documentation
kind=Snippet
detail=library VaultMath
sort_text=0
text_edit=edit 4:0-4:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_special_functions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            constructor(uint256 ownerSeed, address) {}
            ///$2
            fallback(bytes calldata input) external returns (bytes memory output) {
                output = input;
            }
            ///$3
            receive() external payable {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec constructor documentation
kind=Snippet
detail=constructor
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param ownerSeed $2$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec fallback documentation
kind=Snippet
detail=fallback
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param input $2
    /// @return output $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec receive documentation
kind=Snippet
detail=receive
sort_text=0
text_edit=edit 7:4-7:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_events_errors_structs_and_enums() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            event Transfer(address indexed from, address indexed, uint256 amount);
            ///$2
            error TransferFailed(uint256 code, address);
            ///$3
            struct Record {
                uint256 amount;
                address owner;
            }
            ///$4
            enum Status { Pending, Complete }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec event documentation
kind=Snippet
detail=event Transfer
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param from $2
    /// @param amount $3$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec error documentation
kind=Snippet
detail=error TransferFailed
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param code $2$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec struct documentation
kind=Snippet
detail=struct Record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @param owner $3$0

"#]],
    );
    fixture.check_completion_details(
        "$4",
        str![[r#"
label=NatSpec enum documentation
kind=Snippet
detail=enum Status
sort_text=0
text_edit=edit 10:4-10:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_state_variables_and_getter_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
                uint256[] samples;
                mapping(address account => uint256 balance) balances;
            }
            ///$1
            uint256 public total;
            ///$2
            Record public record;
            ///$3
            uint256 private secret;
            ///$4
            uint256 internal cached;
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable total
sort_text=0
text_edit=edit 7:4-7:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return $2$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 9:4-9:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return amount $2
    /// @return owner $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec private state variable documentation
kind=Snippet
detail=private state variable secret
sort_text=0
text_edit=edit 11:4-11:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
    fixture.check_completion_details(
        "$4",
        str![[r#"
label=NatSpec internal state variable documentation
kind=Snippet
detail=internal state variable cached
sort_text=0
text_edit=edit 13:4-13:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
}

#[test]
fn completes_full_and_inheritdoc_templates_for_multiple_bases() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface First { function value(uint256 amount) external view returns (uint256 total); }
        interface Second { function value(uint256 amount) external view returns (uint256 total); }
        contract Child is First, Second {
            ///$1
            function value(uint256 amount)
                external
                pure
                override(First, Second)
                returns (uint256 total)
            {
                total = amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3$0

label=NatSpec @inheritdoc First
kind=Snippet
detail=Inherit documentation from First
sort_text=1:First
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// @inheritdoc First$0

label=NatSpec @inheritdoc Second
kind=Snippet
detail=Inherit documentation from Second
sort_text=1:Second
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Second$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_for_overridden_fallback() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract Base { fallback() external virtual {} }
        contract Child is Base {
            ///$1
            fallback() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec fallback documentation
kind=Snippet
detail=fallback
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Base$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_with_a_named_import_alias() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Base.sol
        interface Original {
            function value() external view returns (uint256 result);
        }

        //- /Completion.sol open
        import {Original as Alias} from "./Base.sol";
        contract Child is Alias {
            ///$1
            function value() external pure override returns (uint256 result) {
                result = 1;
            }
        }
        "#,
        &["/Completion.sol"],
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1
    /// @return result $2$0

label=NatSpec @inheritdoc Alias
kind=Snippet
detail=Inherit documentation from Alias
sort_text=1:Alias
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Alias$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_with_a_reexported_import_alias() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        interface Original { function value() external; }

        //- /Middle.sol
        import {Original as Alias} from "./Base.sol";

        //- /Completion.sol open
        import "./Middle.sol";
        contract Child is Alias {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Alias
kind=Snippet
detail=Inherit documentation from Alias
sort_text=1:Alias
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Alias$0

"#]],
    );
}

#[test]
fn omits_inheritdoc_for_a_base_function_with_a_different_signature() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        interface Base { function value(uint256 amount) external; }
        contract Child is Base {
            ///$1
            function value(address account) external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param account $2$0

"#]],
    );
}

#[test]
fn preserves_dollar_identifiers_in_plain_text_completion() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 $amount) external pure returns (uint256 $result) {
                $result = $amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_snippets(
        "$1",
        false,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=PlainText
new_text:
///
    /// @param $amount
    /// @return $result

"#]],
    );
}

#[test]
fn completes_closed_and_unclosed_block_natspec() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        /**$1 */
        contract Vault {}
        /**$2
        contract OpenVault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:6
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract OpenVault
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn falls_back_to_ordinary_completion_after_closed_block_natspec() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /** docs */ contract C { function f() external pure { ret$1urn; } }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
revert Function

"#]],
    );
}

#[test]
fn completes_closed_block_natspec_before_same_line_declaration() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /**$1 */ contract C {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract C
sort_text=0
text_edit=edit 0:0-0:6
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn completes_multiline_block_natspec_with_non_overlapping_edits() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /**$1
         *
         */
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
additional_text_edit=0:3-2:3 new_text=""
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn current_vfs_syntax_wins_over_stale_state_variable_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            uint256 public value;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("public", "private");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec private state variable documentation
kind=Snippet
detail=private state variable value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
}

#[test]
fn pending_analysis_omits_stale_getter_returns_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            ///$1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("owner", "admin");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// @notice $1$0

"#]],
    );
}

#[test]
fn pending_analysis_omits_stale_inheritdoc_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface First { function value() external; }
        interface Other { function value() external; }
        contract Child is First {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed =
        fixture.project_contents("/Completion.sol").replace("Child is First", "Child is Other");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn pending_context_change_omits_inheritdoc_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_after_context_change(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn pending_trivia_only_change_keeps_getter_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return amount $2
    /// @return owner $3$0

"#]],
    );
}

#[test]
fn pending_unclosed_block_keeps_getter_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "/**");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/**
     * @notice $1
     * @return amount $2
     * @return owner $3$0
     */

"#]],
    );
}

#[test]
fn pending_unclosed_block_keeps_inheritdoc_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "/**");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/**
     * $1$0
     */

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/**
     * @inheritdoc Base$0
     */

"#]],
    );
}

#[test]
fn pending_trivia_only_change_keeps_inheritdoc_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Base$0

"#]],
    );
}

#[test]
fn pending_imported_struct_change_omits_stale_getter_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol open
        struct Record {
            uint256 amount;
            address owner;
        }

        //- /Completion.sol open
        import {Record} from "./Base.sol";
        contract C {
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let base = fixture.project_contents("/Base.sol").replace("owner", "admin");
    let completion = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_changes(
        "$1",
        "/Completion.sol",
        &[("/Base.sol", &base), ("/Completion.sol", &completion)],
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @notice $1$0

"#]],
    );
}

#[test]
fn pending_base_signature_change_omits_stale_inheritdoc() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol open
        interface Base { function value() external; }

        //- /Completion.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let base = fixture.project_contents("/Base.sol").replace("value", "other");
    let completion = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_changes(
        "$1",
        "/Completion.sol",
        &[("/Base.sol", &base), ("/Completion.sol", &completion)],
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn deleted_base_source_omits_stale_inheritdoc() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        interface Base { function value() external; }

        //- /Completion.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_after_deleted_source(
        "$1",
        "/Completion.sol",
        "/Base.sol",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn falls_back_to_plain_text_natspec_when_snippets_are_unsupported() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_snippets(
        "$1",
        false,
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=PlainText
new_text:
/// @title
/// @author
/// @notice

"#]],
    );
}

#[test]
fn rejects_invalid_nonempty_separated_and_unsupported_natspec_targets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ////$1
        contract FourSlashes {}
        /**/$2
        contract EmptyBlock {}
        /***/$3
        contract ThreeStars {}
        /// existing documentation$4
        contract NonEmpty {}
        ///$5
        // intervening comment
        contract Separated {}
        contract C {
            ///$6
            modifier onlyOwner() { _; }
        }
        ///$7
        type Price is uint256;
        "#,
        "/Completion.sol",
    );

    for marker in ["$1", "$2", "$3", "$4", "$5", "$6", "$7"] {
        fixture.check_completion_details(marker, str![""]);
    }
}

#[test]
fn comment_triggers_complete_natspec_templates() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        ///$1
        contract LineDocs {}
        /**$2
        contract BlockDocs {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_trigger(
        "$1",
        "/",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract LineDocs
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details_with_trigger(
        "$2",
        "*",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract BlockDocs
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn comment_triggers_outside_natspec_do_not_leak_symbol_completions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            //$1
            function first() external {}
            //*$2
            function second() external {}
            /*$3 */
            function third() external {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_trigger("$1", "/", str![""]);
    fixture.check_completion_details_with_trigger("$2", "*", str![""]);
    fixture.check_completion_details_with_trigger("$3", "*", str![""]);
}

#[test]
fn completes_symbols_in_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol open
        contract C {
            uint256 stateValue;

            function target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = $1localValue;
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
gasleft Function
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
output Variable
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property
target Method
tx Module

"#]],
    );
}

#[test]
fn filters_locals_by_declaration_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            function f(uint256 input) public pure {
                uint256 localValue = $1input + 1;
                uint256 nextValue = $2localValue;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
    );
}

#[test]
fn completes_dirty_members_from_typed_receivers() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract Token {
            uint256 public balance;
        }

        contract C {
            Token[] tokens;
            Token public token;
            Token foo;

            function getToken() public view returns (Token) {
                return token;
            }

            function read(uint256 i) public view {
                getToken().$1;
                (this.token()).$2b;
                tokens[i].bal$3;
                foo.$4;
                foo
                    .bal$5;
            }
        }
        "#,
        "/Completion.sol",
    );
    let expected = str![[r#"
balance Method

"#]];

    fixture.check_completion("$1", expected.clone());
    fixture.check_completion("$2", expected.clone());
    fixture.check_completion("$3", expected.clone());
    fixture.check_completion("$4", expected.clone());
    fixture.check_completion("$5", expected);
}

#[test]
fn completes_builtin_members_and_filters_globals() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            function f() public view {
                msg.$1;
                tx.$2;
                tx.$3
                block.$4;
                abi.$5;
                ms$6;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
data Method
gas Method
sender Method
sig Method
value Method

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
gasprice Method
origin Method

"#]],
    );
    fixture.check_completion(
        "$3",
        str![[r#"
gasprice Function
origin Function

"#]],
    );
    fixture.check_completion(
        "$4",
        str![[r#"
basefee Function
blobbasefee Function
chainid Function
coinbase Function
difficulty Function
gaslimit Function
number Function
prevrandao Function
timestamp Function

"#]],
    );
    fixture.check_completion(
        "$5",
        str![[r#"
decode Method
encode Method
encodeCall Method
encodePacked Method
encodeWithSelector Method
encodeWithSignature Method

"#]],
    );
    fixture.check_completion(
        "$6",
        str![[r#"
msg Module

"#]],
    );
}

#[test]
fn completes_partial_member_prefixes_from_vfs_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            struct Data {
                uint256 field;
                uint256 other;
            }

            function f() public pure {
                Data memory data;
                data.$1;
                data.f$2;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
field Property
other Property

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
field Property

"#]],
    );
}
