use super::{AnalysisBatch, GlobalState, SymbolTables, analyze, support::RequestFixture};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use lsp_types::{
    DocumentHighlightKind, DocumentHighlightParams, PartialResultParams, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use snapbox::str;
use solar_config::CompileOpts;
use solar_interface::data_structures::map::FxHashSet;
use std::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Waker},
};

#[test]
fn classifies_reads_writes_and_nested_lvalues() {
    let fixture = RequestFixture::new(
        r#"
        //- /Kinds.sol
        contract C {
            struct Box { uint256 $2field; }

            uint256 $1value;
            uint256 $4index;
            mapping(uint256 => Box) $3boxes;
            mapping(uint256 => uint256) $5items;

            function update() external returns (uint256) {
                uint256 read = value;
                value = read;
                value += 1;
                delete value;
                ++value;
                value--;
                boxes[index].field += 1;
                items[index] = value;
                return value;
            }
        }
        "#,
        "/Kinds.sol",
    );

    fixture.check_document_highlights(
        "$1",
        str![[r#"
2:12-2:17 WRITE
7:23-7:28 READ
8:8-8:13 WRITE
9:8-9:13 WRITE
10:15-10:20 WRITE
11:10-11:15 WRITE
12:8-12:13 WRITE
14:23-14:28 READ
15:15-15:20 READ

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
1:25-1:30 WRITE
13:21-13:26 WRITE

"#]],
    );
    fixture.check_document_highlights(
        "$3",
        str![[r#"
4:28-4:33 WRITE
13:8-13:13 READ

"#]],
    );
    fixture.check_document_highlights(
        "$4",
        str![[r#"
3:12-3:17 WRITE
13:14-13:19 READ
14:14-14:19 READ

"#]],
    );
    fixture.check_document_highlights(
        "$5",
        str![[r#"
5:32-5:37 WRITE
14:8-14:13 READ

"#]],
    );
}

#[test]
fn scopes_semantic_matches_to_the_requested_document() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {
            uint256 internal shared;
        }

        //- /Use.sol
        import {Base} from "./Base.sol";
        contract Use is Base {
            function write(uint256 input) external {
                $1shared = input;
            }
            function read() external view returns (uint256) {
                return shared;
            }
            function shadow(uint256 $2shared) external pure returns (uint256) {
                return shared;
            }
        }
        "#,
        "/Use.sol",
    );

    fixture.check_document_highlights(
        "$1",
        str![[r#"
3:8-3:14 WRITE
6:15-6:21 READ

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
8:28-8:34 WRITE
9:15-9:21 READ

"#]],
    );
}

#[test]
fn preserves_ambiguous_reference_targets() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Ambiguous.sol
        contract C {
            function $1pick(uint8 value) internal pure returns (uint8) {
                return value;
            }

            function $2pick(uint256 value) internal pure returns (uint256) {
                return value;
            }

            function call(uint8 value) public pure returns (uint256) {
                return $3pick(value);
            }
        }
        "#,
        "/Ambiguous.sol",
    );

    fixture.check_references(
        "$3",
        true,
        str![[r#"
/Ambiguous.sol:1:13 function pick(uint8 value) internal pure returns (uint8) {
/Ambiguous.sol:4:13 function pick(uint256 value) internal pure returns (uint256) {
/Ambiguous.sol:8:15 return pick(value);

"#]],
    );
    fixture.check_document_highlights(
        "$3",
        str![[r#"
1:13-1:17 WRITE
4:13-4:17 WRITE
8:15-8:19 READ

"#]],
    );
}

#[test]
fn preserves_references_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /First.sol
        contract First {
            uint256 $3value;
            function read() public view returns (uint256) {
                return $4value;
            }
        }

        //- /Second.sol
        contract Second {
            uint256 $1value;
            function write() public {
                $2value = 1;
            }
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Second.sol:1:12 uint256 value;
/Second.sol:3:8 value = 1;

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
1:12-1:17 WRITE
3:8-3:13 WRITE

"#]],
    );
    fixture.check_references(
        "$3",
        true,
        str![[r#"
/First.sol:1:12 uint256 value;
/First.sol:3:15 return value;

"#]],
    );
    fixture.check_document_highlights(
        "$4",
        str![[r#"
1:12-1:17 WRITE
3:15-3:20 READ

"#]],
    );
}

#[test]
fn waits_for_current_analysis_before_returning_highlights() {
    let project = TestProject::from_fixture(
        r#"
        //- /Highlights.sol
        contract C {
            uint256 value;
            function read() external view returns (uint256) {
                return value;
            }
        }
        "#,
    );
    let path = project.path("/Highlights.sol");
    let old_tables = analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: vec![(path.clone(), project.read_file("/Highlights.sol"))],
        seen_paths: FxHashSet::default(),
    })
    .symbol_tables;
    let new_tables = analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: vec![(
            path.clone(),
            "contract C {\n    uint256 placeholder;\n    uint256 value;\n    function write() external {\n        value = 1;\n    }\n}\n".into(),
        )],
        seen_paths: FxHashSet::default(),
    })
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let params = DocumentHighlightParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri),
            position: Position::new(4, 8),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut request = std::pin::pin!(crate::handlers::document_highlight(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, new_tables));
    assert!(!snapshot.publish_symbol_tables(1, SymbolTables::default()));
    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("document-highlight request should complete after analysis is published");
    };
    let highlights = response.unwrap().unwrap();
    assert_eq!(highlights.len(), 2);
    assert_eq!(highlights[0].range.start, Position::new(2, 12));
    assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::WRITE));
    assert_eq!(highlights[1].range.start, Position::new(4, 8));
    assert_eq!(highlights[1].kind, Some(DocumentHighlightKind::WRITE));
}
