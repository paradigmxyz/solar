use super::super::{AnalysisBatch, GlobalState, analyze};
use crate::{handlers, test_support::TestProject};
use async_lsp::ClientSocket;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, PartialResultParams,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};
use snapbox::{IntoData, assert_data_eq};
use solar_config::CompileOpts;
use solar_interface::data_structures::{map::FxHashSet, sync::RwLock};
use std::{fmt::Write, sync::Arc};

#[test]
fn completes_imported_symbols_and_builtins_in_scope() {
    check_completion(
        r#"
        //- /lib/Library.sol
        library MathLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract Base {
            function inherited() internal pure {}
        }

        //- /Completion.sol
        import {MathLib as Lib, Base} from "./lib/Library.sol";

        contract C is Base {
            using Lib for uint256;

            function f(uint256 value) public pure {
                uint256 localValue = value;
                $1localValue;
            }
        }
        "#,
        snapbox::str![[r#"
Base | class | -
C | class | -
Lib | module | MathLib
abi | module | builtin
addmod | function | builtin
assert | function | builtin
blobhash | function | builtin
block | module | builtin
blockhash | function | builtin
ecrecover | function | builtin
erc7201 | function | builtin
f | method | -
gasleft | function | builtin
inherited | method | -
keccak256 | function | builtin
localValue | variable | -
msg | module | builtin
mulmod | function | builtin
require | function | builtin
revert | function | builtin
ripemd160 | function | builtin
selfdestruct | function | builtin
sha256 | function | builtin
super | function | builtin
this | function | builtin
tx | module | builtin
value | variable | -
"#]],
    );
}

#[test]
fn completes_function_scope_symbols() {
    check_completion(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 stateValue;

            function target(uint256 input) public returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = $1localValue;
            }

            function caller() public {
                uint256 callerLocal = target(stateValue);
            }
        }
        "#,
        completion_with_builtins(&[
            "C | class | -",
            "caller | method | -",
            "input | variable | -",
            "localValue | variable | -",
            "output | variable | -",
            "stateValue | property | -",
            "target | method | -",
        ]),
    );
}

#[test]
fn does_not_complete_local_before_declaration_is_in_scope() {
    check_completion(
        r#"
        //- /Completion.sol
        contract C {
            function f(uint256 input) public pure {
                uint256 localValue = $1input + 1;
                uint256 nextValue = localValue;
            }
        }
        "#,
        completion_with_builtins(&["C | class | -", "f | method | -", "input | variable | -"]),
    );

    check_completion(
        r#"
        //- /Completion.sol
        contract C {
            function f(uint256 input) public pure {
                uint256 localValue = input + 1;
                uint256 nextValue = $1localValue;
            }
        }
        "#,
        completion_with_builtins(&[
            "C | class | -",
            "f | method | -",
            "input | variable | -",
            "localValue | variable | -",
        ]),
    );
}

#[test]
fn completes_struct_members_from_receiver_type() {
    check_completion(
        r#"
        //- /Members.sol
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                return data.$1field + value.inc() + uint256(Choice.A);
            }
        }
        "#,
        snapbox::str![[r#"
field | property | -
other | property | -
"#]],
    );
}

#[test]
fn completes_using_members_from_receiver_type() {
    check_completion(
        r#"
        //- /Members.sol
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                return data.field + value.$1inc() + uint256(Choice.A);
            }
        }
        "#,
        snapbox::str![[r#"
inc | method | -
"#]],
    );
}

#[test]
fn completes_enum_members_from_receiver_type() {
    check_completion(
        r#"
        //- /Members.sol
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                return data.field + value.inc() + uint256(Choice.$1A);
            }
        }
        "#,
        snapbox::str![[r#"
A | enum member | Choice
B | enum member | Choice
"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn completes_members_after_trailing_dot_in_open_document() {
    check_dirty_completion(
        r#"
        //- /Members.sol open
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                data.$1
                value.
                Choice.
            }
        }
        "#,
        |dirty_source| {
            dirty_source
                .replace("data.", "data.field;")
                .replace("value.", "value.inc();")
                .replace("Choice.", "Choice.A;")
        },
        snapbox::str![[r#"
field | property | -
other | property | -
"#]],
    )
    .await;

    check_dirty_completion(
        r#"
        //- /Members.sol open
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                data.
                value.$1
                Choice.
            }
        }
        "#,
        |dirty_source| {
            dirty_source
                .replace("data.", "data.field;")
                .replace("value.", "value.inc();")
                .replace("Choice.", "Choice.A;")
        },
        snapbox::str![[r#"
inc | method | -
"#]],
    )
    .await;

    check_dirty_completion(
        r#"
        //- /Members.sol open
        library UIntLib {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        contract C {
            using UIntLib for uint256;

            enum Choice { A, B }
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data, uint256 value) public pure returns (uint256) {
                data.
                value.
                Choice.$1
            }
        }
        "#,
        |dirty_source| {
            dirty_source
                .replace("data.", "data.field;")
                .replace("value.", "value.inc();")
                .replace("Choice.", "Choice.A;")
        },
        snapbox::str![[r#"
A | enum member | Choice
B | enum member | Choice
"#]],
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn completes_members_from_live_line_in_open_document() {
    check_dirty_completion(
        r#"
        //- /Members.sol open
        contract C {
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data) public pure {
                data.$1;
            }
        }
        "#,
        |dirty_source| dirty_source.replace("data.", "data"),
        snapbox::str![[r#"
field | property | -
other | property | -
"#]],
    )
    .await;

    check_dirty_completion(
        r#"
        //- /Members.sol open
        contract C {
            struct Data { uint256 field; uint256 other; }

            function read(Data memory data) public pure {
                data.f$1;
            }
        }
        "#,
        |dirty_source| dirty_source.replace("data.f", "data"),
        snapbox::str![[r#"
field | property | -
other | property | -
"#]],
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn completes_builtin_module_members_from_live_line_in_open_document() {
    check_dirty_completion(
        r#"
        //- /Members.sol open
        contract C {
            function read() public payable {
                msg.$1;
            }
        }
        "#,
        |dirty_source| dirty_source.replace("msg.", "uint256 marker = 1"),
        snapbox::str![[r#"
data | method | -
gas | method | -
sender | method | -
sig | method | -
value | method | -
"#]],
    )
    .await;
}

fn check_completion(fixture: &str, expected: impl IntoData) {
    let fixture = TestProject::from_fixture_with_cursor(fixture);
    let cursor = fixture.cursor;
    let result = analyze_fixture(fixture.files);

    let completions = result.symbol_tables.completion_items(&cursor.uri, cursor.position);
    assert_data_eq!(format_completion_items(&completions), expected);
}

async fn check_dirty_completion(
    fixture: &str,
    clean_source: impl FnOnce(&str) -> String,
    expected: impl IntoData,
) {
    let fixture = TestProject::from_fixture_with_cursor(fixture);
    let dirty_source = fixture
        .files
        .iter()
        .find_map(|(path, contents)| (path == &fixture.cursor.path).then_some(contents.as_str()))
        .expect("cursor file must be present in fixture");
    let clean_source = clean_source(dirty_source);
    let result = analyze_fixture([(fixture.cursor.path.clone(), clean_source)]);

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.vfs = Arc::new(RwLock::new(fixture.project.vfs()));
    *state.symbol_tables.write() = result.symbol_tables;

    let response = handlers::completion(
        &mut state,
        CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: fixture.cursor.uri },
                position: fixture.cursor.position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        },
    )
    .await
    .unwrap()
    .unwrap();

    let CompletionResponse::Array(completions) = response else {
        panic!("expected completion array response");
    };
    assert_data_eq!(format_completion_items(&completions), expected);
}

fn analyze_fixture(
    files: impl IntoIterator<Item = (std::path::PathBuf, String)>,
) -> super::super::AnalysisResult {
    let result = analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: files.into_iter().collect(),
        seen_paths: FxHashSet::default(),
    });
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    result
}

fn format_completion_items(completions: &[CompletionItem]) -> String {
    let mut output = String::new();
    for item in completions {
        if !output.is_empty() {
            output.push('\n');
        }
        let detail = item.detail.as_deref().unwrap_or("-");
        write!(&mut output, "{} | {} | {detail}", item.label, completion_item_kind(item.kind),)
            .unwrap();
    }
    output
}

fn completion_with_builtins(scope_items: &[&str]) -> String {
    let mut items = Vec::with_capacity(scope_items.len() + BUILTIN_COMPLETIONS.len());
    items.extend_from_slice(scope_items);
    items.extend_from_slice(BUILTIN_COMPLETIONS);
    items.sort_unstable();
    items.join("\n")
}

const BUILTIN_COMPLETIONS: &[&str] = &[
    "abi | module | builtin",
    "addmod | function | builtin",
    "assert | function | builtin",
    "blobhash | function | builtin",
    "block | module | builtin",
    "blockhash | function | builtin",
    "ecrecover | function | builtin",
    "erc7201 | function | builtin",
    "gasleft | function | builtin",
    "keccak256 | function | builtin",
    "msg | module | builtin",
    "mulmod | function | builtin",
    "require | function | builtin",
    "revert | function | builtin",
    "ripemd160 | function | builtin",
    "selfdestruct | function | builtin",
    "sha256 | function | builtin",
    "super | function | builtin",
    "this | function | builtin",
    "tx | module | builtin",
];

fn completion_item_kind(kind: Option<CompletionItemKind>) -> &'static str {
    let Some(kind) = kind else {
        return "-";
    };

    if kind == CompletionItemKind::CLASS {
        "class"
    } else if kind == CompletionItemKind::CONSTANT {
        "constant"
    } else if kind == CompletionItemKind::ENUM {
        "enum"
    } else if kind == CompletionItemKind::ENUM_MEMBER {
        "enum member"
    } else if kind == CompletionItemKind::FIELD {
        "field"
    } else if kind == CompletionItemKind::FUNCTION {
        "function"
    } else if kind == CompletionItemKind::METHOD {
        "method"
    } else if kind == CompletionItemKind::MODULE {
        "module"
    } else if kind == CompletionItemKind::PROPERTY {
        "property"
    } else if kind == CompletionItemKind::STRUCT {
        "struct"
    } else if kind == CompletionItemKind::TYPE_PARAMETER {
        "type parameter"
    } else if kind == CompletionItemKind::VARIABLE {
        "variable"
    } else {
        "text"
    }
}
