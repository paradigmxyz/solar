use super::{AnalysisBatch, GlobalState, SymbolTables, analyze, support::RequestFixture};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use snapbox::str;
use solar_config::CompileOpts;
use std::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Waker},
};

#[test]
fn resolves_struct_type_from_variable_declaration() {
    let fixture = RequestFixture::new(
        r#"
        //- /TypeDefinition.sol
        contract C {
            struct Data { uint256 value; }
            Data $1data;
        }
        "#,
        "/TypeDefinition.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/TypeDefinition.sol:1:11 struct Data { uint256 value; }

"#]],
    );
}

#[test]
fn user_defined_type_declarations_resolve_to_themselves() {
    let fixture = RequestFixture::new(
        r#"
        //- /Types.sol
        interface $1InterfaceType {}
        library $2LibraryType {}
        contract $3ContractType {}
        struct $4StructType { uint256 value; }
        enum $5EnumType { A }
        type $6ValueType is uint256;
        "#,
        "/Types.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/Types.sol:0:10 interface InterfaceType {}

"#]],
    );
    fixture.check_goto_type_definition(
        "$2",
        str![[r#"
/Types.sol:1:8 library LibraryType {}

"#]],
    );
    fixture.check_goto_type_definition(
        "$3",
        str![[r#"
/Types.sol:2:9 contract ContractType {}

"#]],
    );
    fixture.check_goto_type_definition(
        "$4",
        str![[r#"
/Types.sol:3:7 struct StructType { uint256 value; }

"#]],
    );
    fixture.check_goto_type_definition(
        "$5",
        str![[r#"
/Types.sol:4:5 enum EnumType { A }

"#]],
    );
    fixture.check_goto_type_definition(
        "$6",
        str![[r#"
/Types.sol:5:5 type ValueType is uint256;

"#]],
    );
}

#[test]
fn resolves_variable_declarations_and_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Variables.sol
        struct Data { uint256 value; }
        contract C {
            Data $1stored;

            function use(Data memory $2input) public {
                Data memory $3local = $4input;
                $5stored = $6local;
            }
        }
        "#,
        "/Variables.sol",
    );
    let expected = str![[r#"
/Variables.sol:0:7 struct Data { uint256 value; }

"#]];

    for marker in ["$1", "$2", "$3", "$4", "$5", "$6"] {
        fixture.check_goto_type_definition(marker, expected.clone());
    }
}

#[test]
fn unwraps_arrays_and_mapping_values() {
    let fixture = RequestFixture::new(
        r#"
        //- /Containers.sol
        enum Key { A }
        struct Value { uint256 value; }
        contract C {
            Value[][] $1nested;
            mapping(Key => Value[]) $2values;
        }
        "#,
        "/Containers.sol",
    );
    let expected = str![[r#"
/Containers.sol:1:7 struct Value { uint256 value; }

"#]];

    fixture.check_goto_type_definition("$1", expected.clone());
    fixture.check_goto_type_definition("$2", expected);
}

#[test]
fn function_targets_preserve_return_order_and_stably_deduplicate() {
    let fixture = RequestFixture::new(
        r#"
        //- /Returns.sol
        contract C {
            struct Second { uint256 value; }
            struct First { uint256 value; }

            function $1pair() public pure returns (First memory, Second memory, First memory) {
                revert();
            }

            function use() public pure {
                $2pair();
            }
        }
        "#,
        "/Returns.sol",
    );
    let expected = str![[r#"
/Returns.sol:2:11 struct First { uint256 value; }
/Returns.sol:1:11 struct Second { uint256 value; }

"#]];

    fixture.check_goto_type_definition("$1", expected.clone());
    fixture.check_goto_type_definition("$2", expected);
}

#[test]
fn overloaded_calls_use_the_selected_function_return_type() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            struct NumberResult { uint256 value; }
            struct TextResult { string value; }

            function pick(uint256) public pure returns (NumberResult memory) { revert(); }
            function pick(string memory) public pure returns (TextResult memory) { revert(); }

            function use() public pure {
                $1pick(uint256(1));
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/Overload.sol:1:11 struct NumberResult { uint256 value; }

"#]],
    );
}

#[test]
fn public_mapping_getters_use_the_source_variable_type() {
    let fixture = RequestFixture::new(
        r#"
        //- /Getter.sol
        struct Data { uint256 value; }
        contract C {
            mapping(uint256 => Data) public values;

            function read(uint256 key) external view {
                this.$1values(key);
            }
        }
        "#,
        "/Getter.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/Getter.sol:0:7 struct Data { uint256 value; }

"#]],
    );
}

#[test]
fn resolves_cross_file_and_late_declared_types() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol
        import {Shared} from "./Types.sol";
        import "./Late.sol";
        contract Main { Shared $1value; }

        //- /Types.sol
        struct Shared { uint256 value; }

        //- /Late.sol
        contract UsesLater {
            Later $2value;
        }
        struct Later { uint256 value; }
        "#,
        "/Main.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/Types.sol:0:7 struct Shared { uint256 value; }

"#]],
    );
    fixture.check_goto_type_definition(
        "$2",
        str![[r#"
/Late.sol:3:7 struct Later { uint256 value; }

"#]],
    );
}

#[test]
fn preserves_type_definitions_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /first/Types.sol
        struct FirstType { uint256 value; }

        //- /first/Main.sol
        import "./Types.sol";
        contract First { FirstType $1value; }

        //- /second/Types.sol
        struct SecondType { uint256 value; }

        //- /second/Main.sol
        import "./Types.sol";
        contract Second { SecondType $2value; }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/first/Types.sol:0:7 struct FirstType { uint256 value; }

"#]],
    );
    fixture.check_goto_type_definition(
        "$2",
        str![[r#"
/second/Types.sol:0:7 struct SecondType { uint256 value; }

"#]],
    );
}

#[test]
fn primitive_function_and_unresolved_types_have_no_target() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /NoTarget.sol
        struct Detail { uint256 code; }
        error $5Error(Detail detail);
        contract C {
            event $4Event(Detail detail);
            uint256 $1count;

            function use(function(uint256) external returns (uint256) $2callback) public {
                Missing $3missing;
            }

            modifier $6Modifier(Detail detail) {
                _;
            }
        }
        "#,
        "/NoTarget.sol",
    );

    for marker in ["$1", "$2", "$3", "$4", "$5", "$6"] {
        fixture.check_goto_type_definition(marker, "<none>\n");
    }
}

#[test]
fn resolves_named_custom_error_parameter_types() {
    let fixture = RequestFixture::new(
        r#"
        //- /Error.sol
        struct Detail { uint256 code; }
        error Failed(Detail detail);

        contract C {
            function fail() external pure {
                revert Failed({ $1detail: Detail({ code: 1 }) });
            }
        }
        "#,
        "/Error.sol",
    );

    fixture.check_goto_type_definition(
        "$1",
        str![[r#"
/Error.sol:0:7 struct Detail { uint256 code; }

"#]],
    );
}

#[test]
fn waits_for_current_analysis_before_returning_type_definitions() {
    let project = TestProject::from_fixture(
        r#"
        //- /Types.sol
        contract C {
            struct OldType { uint256 value; }
            struct Placeholder { uint256 value; }
            OldType $1value;
        }
        "#,
    );
    let path = project.path("/Types.sol");
    let old_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Types.sol"))],
    ))
    .symbol_tables;
    let new_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(
            path.clone(),
            "contract C {\n    struct Placeholder { uint256 value; }\n    struct NewType { uint256 value; }\n    NewType value;\n}\n".into(),
        )],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position: Position::new(3, 12),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut request = std::pin::pin!(crate::handlers::goto_type_definition(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, new_tables));
    assert!(!snapshot.publish_symbol_tables(1, SymbolTables::default()));
    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("type-definition request should complete after analysis is published");
    };
    let Some(GotoDefinitionResponse::Array(locations)) = response.unwrap() else {
        panic!("expected type-definition locations");
    };
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(2, 11));
}
