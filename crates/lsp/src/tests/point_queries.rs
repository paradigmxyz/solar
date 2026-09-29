use super::*;
use lsp_types::GotoDefinitionResponse;
use snapbox::{assert_data_eq, str};
use std::fmt::Write as _;

fn remapped_guard_project() -> MarkedProject {
    MarkedProject::from_fixture(
        r#"
        //- /left/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@auth/=lib/auth/", "shared/=../shared/"]

        //- /left/src/Main.sol
        import "shared/Shared.sol";

        //- /left/lib/auth/Guard.sol
        /// @notice Shared guard documentation.
        contract Guard {}

        //- /right/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@auth/=lib/auth/", "shared/=../shared/"]

        //- /right/src/Main.sol
        import "shared/Shared.sol";

        //- /right/lib/auth/Guard.sol
        /// @notice Shared guard documentation.
        contract Guard {}

        //- /shared/Shared.sol
        import {Guard} from "@auth/Guard.sol";
        import {Guard as LeftGuard} from "../left/lib/auth/Guard.sol";
        contract Shared is $1Guard {
            $2Guard $3value;
            $5LeftGuard fixedValue;

            function read() external view returns (Guard) {
                return $4value;
            }
        }
        "#,
    )
}

fn analyze_roots(marked: &MarkedProject, roots: &[&str]) -> SymbolTables {
    let project = marked.project();
    let snapshot = snapshot_with_config(project.config_with_roots(roots), project.vfs());
    let batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), roots.len(), "one analysis batch per Foundry workspace");
    let mut results = AnalysisResultAccumulator::default();
    for batch in batches {
        let result = analyze(batch);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        results.push(result);
    }
    results.finish().symbol_tables
}

const DEFINITION: usize = 0;
const TYPE_DEFINITION: usize = 2;
const HOVER: usize = 3;
const HIGHLIGHTS: usize = 4;

/// Formats the definition, declaration, type-definition, hover, and highlight answers.
fn point_queries(tables: &SymbolTables, uri: &Url, position: Position) -> [Option<String>; 5] {
    fn debug(response: Option<impl std::fmt::Debug>) -> Option<String> {
        response.map(|response| format!("{response:?}"))
    }
    [
        debug(tables.goto_definition(uri, position)),
        debug(tables.goto_declaration(uri, position)),
        debug(tables.goto_type_definition(uri, position)),
        debug(tables.hover(uri, position)),
        debug(tables.document_highlights(uri, position)),
    ]
}

fn assert_no_point_query(tables: &SymbolTables, uri: &Url, position: Position) {
    assert_eq!(point_queries(tables, uri, position), <[Option<String>; 5]>::default());
}

/// Asserts that `tables` keeps the baseline's declarations but rejects its type definition.
fn assert_rejects_type_definition(
    tables: &SymbolTables,
    baseline: &SymbolTables,
    uri: &Url,
    position: Position,
) {
    let mut expected = point_queries(baseline, uri, position);
    assert!(expected[DEFINITION].is_some());
    assert!(expected[TYPE_DEFINITION].take().is_some());
    let actual = point_queries(tables, uri, position);
    assert_eq!(actual[..=TYPE_DEFINITION], expected[..=TYPE_DEFINITION]);
}

/// Analyzes each file as its own batch, returning the first batch's tables and the merged tables.
fn analyze_files(
    files: impl IntoIterator<Item = (PathBuf, String)>,
    diagnostics: bool,
) -> (SymbolTables, SymbolTables) {
    let mut results = AnalysisResultAccumulator::default();
    let mut first = None;
    for file in files {
        let result = analyze_source(file.0, file.1);
        assert_eq!(result.diagnostics.is_empty(), !diagnostics, "{:#?}", result.diagnostics);
        first.get_or_insert_with(|| result.symbol_tables.clone());
        results.push(result);
    }
    (first.unwrap(), results.finish().symbol_tables)
}

#[test]
fn incompatible_contexts_fail_closed_in_both_workspace_orders() {
    let marked = remapped_guard_project();
    let uri = marked.project().uri("/shared/Shared.sol");
    let position = |marker| marked.marker(marker).position();
    let left = analyze_roots(&marked, &["/left"]);
    let right = analyze_roots(&marked, &["/right"]);
    let left_highlights = left.document_highlights(&uri, position("$5")).unwrap();
    let right_highlights = right.document_highlights(&uri, position("$5")).unwrap();
    assert!(left_highlights.len() > right_highlights.len());

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        // Incompatible remappings reject every query at the shared `Guard` references.
        for marker in ["$1", "$2"] {
            assert_no_point_query(&tables, &uri, position(marker));
        }
        // Shared variables keep their source declaration but not their resolved types.
        for marker in ["$3", "$4"] {
            assert_rejects_type_definition(&tables, &left, &uri, position(marker));
        }
        // A fixed alias keeps its definition, but its occurrences differ between contexts.
        let position = position("$5");
        assert_eq!(tables.goto_definition(&uri, position), left.goto_definition(&uri, position));
        assert_eq!(tables.document_highlights(&uri, position), None);
    }
}

#[test]
fn compatible_contexts_preserve_and_deduplicate_point_queries() {
    let marked = remapped_guard_project();
    marked.project().write_file(
        "/right/foundry.toml",
        concat!(
            "[profile.default]\n",
            "auto_detect_remappings = false\n",
            "remappings = [\"@auth/=../left/lib/auth/\", \"shared/=../shared/\"]\n",
        ),
    );
    let uri = marked.project().uri("/shared/Shared.sol");
    let baseline = analyze_roots(&marked, &["/left"]);

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        for marker in ["$1", "$2", "$3", "$4", "$5"] {
            let position = marked.marker(marker).position();
            let expected = point_queries(&baseline, &uri, position);
            for query in [DEFINITION, HOVER, HIGHLIGHTS] {
                assert!(expected[query].is_some(), "{marker} {query}");
            }
            // The baseline has one definition and at least one highlight.
            assert!(
                matches!(
                    baseline.goto_definition(&uri, position),
                    Some(GotoDefinitionResponse::Array(locations)) if locations.len() == 1
                ),
                "{marker}"
            );
            assert!(baseline.document_highlights(&uri, position).is_some_and(|h| !h.is_empty()));
            assert_eq!(point_queries(&tables, &uri, position), expected, "{marker}");
        }
    }
}

#[test]
fn shared_function_hover_compares_inherited_documentation_in_both_orders() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /left/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@dep/=lib/dep/", "shared/=../shared/"]

        //- /left/src/Main.sol
        import "shared/Shared.sol";

        //- /left/lib/dep/Base.sol
        abstract contract Base {
            /// @notice Documentation from the left context.
            function documented() public pure virtual returns (uint256);
        }

        //- /right/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@dep/=lib/dep/", "shared/=../shared/"]

        //- /right/src/Main.sol
        import "shared/Shared.sol";

        //- /right/lib/dep/Base.sol
        abstract contract Base {
            /// @notice Documentation from the right context.
            function documented() public pure virtual returns (uint256);
        }

        //- /shared/Shared.sol
        import {Base} from "@dep/Base.sol";
        contract Shared is Base {
            /// @inheritdoc Base
            function $1documented() public pure override returns (uint256) {
                return 1;
            }

            function use() external pure returns (uint256) {
                return $2documented();
            }
        }
        "#,
    );
    let uri = marked.project().uri("/shared/Shared.sol");
    let left = analyze_roots(&marked, &["/left"]);
    let right = analyze_roots(&marked, &["/right"]);
    for marker in ["$1", "$2"] {
        let position = marked.marker(marker).position();
        assert!(left.hover(&uri, position).is_some());
        assert!(right.hover(&uri, position).is_some());
        assert_ne!(left.hover(&uri, position), right.hover(&uri, position));
    }

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        for marker in ["$1", "$2"] {
            let position = marked.marker(marker).position();
            let definition = left.goto_definition(&uri, position);
            assert!(definition.is_some());
            assert_eq!(tables.goto_definition(&uri, position), definition);
            assert_eq!(tables.hover(&uri, position), None, "{marker}");
        }
    }
}

#[test]
fn conflicting_source_snapshots_fail_closed_in_both_batch_orders() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Shared.sol
        contract $1Shared {
            function use() external pure returns (Shared) {
                return $2Shared(address(0));
            }
        }
        "#,
    );
    let path = marked.project().path("/Shared.sol");
    let uri = marked.project().uri("/Shared.sol");
    let current = marked.project().read_file("/Shared.sol");
    // Keep queried ranges identical so rejecting conflicting snapshots cannot rely on offsets.
    let changed = current.replace("address(0)", "address(1)");

    for sources in [[&current, &changed], [&changed, &current]] {
        let (_, tables) =
            analyze_files(sources.map(|source| (path.clone(), source.clone())), false);
        for marker in ["$1", "$2"] {
            assert_no_point_query(&tables, &uri, marked.marker(marker).position());
        }
    }
}

#[test]
fn conflicting_target_snapshots_reject_direct_and_projected_targets_in_both_orders() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Types.sol
        contract Target { uint256 value; }

        //- /Shared.sol
        import {Target} from "./Types.sol";
        contract Shared {
            $1Target $2value;
            function read() external view returns (Target) {
                return $3value;
            }
        }

        //- /left/Main.sol
        import "../Shared.sol";

        //- /right/Main.sol
        import "../Shared.sol";
        "#,
    );
    let project = marked.project();
    let uri = project.uri("/Shared.sol");
    let current_target = project.read_file("/Types.sol");
    let changed_target = current_target.replace("uint256", "bytes32");

    for entries in [
        [("/left/Main.sol", &current_target), ("/right/Main.sol", &changed_target)],
        [("/right/Main.sol", &changed_target), ("/left/Main.sol", &current_target)],
    ] {
        // Each batch reads the target snapshot written just before it is analyzed.
        let files = entries.into_iter().map(|(entry, target_source)| {
            project.write_file("/Types.sol", target_source);
            (project.path(entry), project.read_file(entry))
        });
        let (baseline, tables) = analyze_files(files, false);
        assert_no_point_query(&tables, &uri, marked.marker("$1").position());
        for marker in ["$2", "$3"] {
            assert_rejects_type_definition(
                &tables,
                &baseline,
                &uri,
                marked.marker(marker).position(),
            );
        }
    }
}

#[test]
fn compatible_ambiguous_overloads_keep_all_targets_in_both_batch_orders() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Shared.sol
        contract Shared {
            struct Small { uint256 value; }
            struct Large { uint256 value; }

            function pick(uint8) internal pure returns (Small memory) { revert(); }
            function pick(uint256) internal pure returns (Large memory) { revert(); }

            function use(uint8 value) external pure {
                $1pick(value);
            }
        }

        //- /left/Main.sol
        import "../Shared.sol";

        //- /right/Main.sol
        import "../Shared.sol";
        "#,
    );
    let project = marked.project();
    let uri = project.uri("/Shared.sol");
    let position = marked.marker("$1").position();

    for paths in [["/left/Main.sol", "/right/Main.sol"], ["/right/Main.sol", "/left/Main.sol"]] {
        // The ambiguous call deliberately produces a diagnostic and two navigation targets.
        let files = paths.map(|path| (project.path(path), project.read_file(path)));
        let (baseline, tables) = analyze_files(files, true);
        for response in [
            baseline.goto_definition(&uri, position),
            baseline.goto_type_definition(&uri, position),
        ] {
            let Some(GotoDefinitionResponse::Array(locations)) = response else {
                panic!("expected ambiguous navigation targets");
            };
            assert_eq!(locations.len(), 2);
        }
        let expected = point_queries(&baseline, &uri, position);
        assert_eq!(expected[HOVER], None);
        assert_eq!(point_queries(&tables, &uri, position), expected);
    }
}

#[test]
fn requests_wait_for_requested_analysis() {
    let fixture = RequestFixture::new(
        r#"
        //- /Fresh.sol
        contract C {
            struct Placeholder { uint256 value; }
            struct NewType { uint256 value; }
            NewType $1value;
            function write() external {
                value = NewType(1);
            }
        }
        "#,
        "/Fresh.sol",
    );
    let old_source = "contract C {\n    uint256 oldValue;\n}\n";
    let old_tables = analyze_source(fixture.project_path("/Fresh.sol"), old_source).symbol_tables;
    let (uri, position) = fixture.marker_location("$1");
    let mut output = String::new();
    for query in Query::ALL {
        let mut state = fixture.state();
        let new_tables = state.symbol_tables.load_full();
        state.symbol_tables.store(Arc::new(old_tables.clone()));
        state.mark_analysis_pending_for_test();
        let mut request = start_request(query.request(&mut state, uri.clone(), position));

        let mut snapshot = state.snapshot();
        assert!(snapshot.publish_symbol_tables(1, new_tables));
        assert!(!snapshot.publish_symbol_tables(0, Default::default()));
        let response = expect_ready(request.as_mut()).unwrap();
        write!(output, "{}: {}", query.label(), fixture.response_output(response)).unwrap();
    }
    assert_data_eq!(
        output,
        str![[r#"
definition: /Fresh.sol:3:12 NewType value;
declaration: /Fresh.sol:3:12 NewType value;
implementation: /Fresh.sol:3:12 NewType value;
type definition: /Fresh.sol:2:11 struct NewType { uint256 value; }
references: /Fresh.sol:3:12 NewType value;
/Fresh.sol:5:8 value = NewType(1);
highlights: 3:12-3:17 WRITE
5:8-5:13 WRITE
hover: 3:12-3:17 NewType value

"#]]
    );
}
