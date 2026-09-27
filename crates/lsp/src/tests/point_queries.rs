use super::{
    AnalysisBatch, AnalysisResultAccumulator, SymbolTables, analyze, snapshot_with_config,
};
use crate::test_support::MarkedProject;
use lsp_types::{GotoDefinitionResponse, Position, Url};
use solar_config::CompileOpts;

fn remapped_guard_project() -> MarkedProject {
    MarkedProject::from_fixture(
        r#"
        //- /left/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@auth/=lib/auth/"]

        //- /left/src/Main.sol
        import "../../shared/Shared.sol";

        //- /left/lib/auth/Guard.sol
        /// @notice Shared guard documentation.
        contract Guard {}

        //- /right/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@auth/=lib/auth/"]

        //- /right/src/Main.sol
        import "../../shared/Shared.sol";

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

fn assert_no_point_query(tables: &SymbolTables, uri: &Url, position: Position) {
    assert_eq!(tables.goto_definition(uri, position), None, "definition");
    assert_eq!(tables.goto_declaration(uri, position), None, "declaration");
    assert_eq!(tables.goto_type_definition(uri, position), None, "type definition");
    assert_eq!(tables.hover(uri, position), None, "hover");
    assert_eq!(tables.document_highlights(uri, position), None, "document highlights");
}

#[test]
fn incompatible_remappings_fail_closed_in_both_workspace_orders() {
    let marked = remapped_guard_project();
    let uri = Url::from_file_path(marked.project().path("/shared/Shared.sol")).unwrap();

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        for marker in ["$1", "$2"] {
            assert_no_point_query(&tables, &uri, marked.marker(marker).position());
        }
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
            "remappings = [\"@auth/=../left/lib/auth/\"]\n",
        ),
    );
    let uri = Url::from_file_path(marked.project().path("/shared/Shared.sol")).unwrap();
    let baseline = analyze_roots(&marked, &["/left"]);

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        for marker in ["$1", "$2", "$3", "$4", "$5"] {
            let position = marked.marker(marker).position();
            let definition = baseline.goto_definition(&uri, position);
            let Some(GotoDefinitionResponse::Array(locations)) = &definition else {
                panic!("expected a definition for {marker}");
            };
            assert_eq!(locations.len(), 1);
            assert_eq!(tables.goto_definition(&uri, position), definition);
            assert_eq!(
                tables.goto_declaration(&uri, position),
                baseline.goto_declaration(&uri, position)
            );
            assert_eq!(
                tables.goto_type_definition(&uri, position),
                baseline.goto_type_definition(&uri, position)
            );
            let hover = baseline.hover(&uri, position);
            assert!(hover.is_some(), "expected hover for {marker}");
            assert_eq!(tables.hover(&uri, position), hover);
            let highlights = baseline.document_highlights(&uri, position);
            assert!(highlights.as_ref().is_some_and(|highlights| !highlights.is_empty()));
            assert_eq!(tables.document_highlights(&uri, position), highlights);
        }
    }
}

#[test]
fn shared_variable_type_definitions_compare_resolved_types_in_both_orders() {
    let marked = remapped_guard_project();
    let uri = Url::from_file_path(marked.project().path("/shared/Shared.sol")).unwrap();
    let baseline = analyze_roots(&marked, &["/left"]);

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        for marker in ["$3", "$4"] {
            let position = marked.marker(marker).position();
            let definition = baseline.goto_definition(&uri, position);
            assert!(definition.is_some());
            assert_eq!(tables.goto_definition(&uri, position), definition);
            assert_eq!(
                tables.goto_declaration(&uri, position),
                baseline.goto_declaration(&uri, position)
            );
            assert!(baseline.goto_type_definition(&uri, position).is_some());
            assert_eq!(tables.goto_type_definition(&uri, position), None, "{marker}");
        }
    }
}

#[test]
fn shared_highlight_targets_reject_incompatible_occurrences_in_both_orders() {
    let marked = remapped_guard_project();
    let uri = Url::from_file_path(marked.project().path("/shared/Shared.sol")).unwrap();
    let position = marked.marker("$5").position();
    let left = analyze_roots(&marked, &["/left"]);
    let right = analyze_roots(&marked, &["/right"]);
    let left_highlights = left.document_highlights(&uri, position).unwrap();
    let right_highlights = right.document_highlights(&uri, position).unwrap();
    assert!(left_highlights.len() > right_highlights.len());

    for roots in [["/left", "/right"], ["/right", "/left"]] {
        let tables = analyze_roots(&marked, &roots);
        assert_eq!(tables.goto_definition(&uri, position), left.goto_definition(&uri, position));
        assert_eq!(tables.document_highlights(&uri, position), None);
    }
}

#[test]
fn shared_function_hover_compares_inherited_documentation_in_both_orders() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /left/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@dep/=lib/dep/"]

        //- /left/src/Main.sol
        import "../../shared/Shared.sol";

        //- /left/lib/dep/Base.sol
        abstract contract Base {
            /// @notice Documentation from the left context.
            function documented() public pure virtual returns (uint256);
        }

        //- /right/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["@dep/=lib/dep/"]

        //- /right/src/Main.sol
        import "../../shared/Shared.sol";

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
    let uri = Url::from_file_path(marked.project().path("/shared/Shared.sol")).unwrap();
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
    let project = marked.project();
    let path = project.path("/Shared.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let current = project.read_file("/Shared.sol");
    // Keep queried ranges identical so rejecting conflicting snapshots cannot rely on offsets.
    let changed = current.replace("address(0)", "address(1)");

    for sources in [[&current, &changed], [&changed, &current]] {
        let mut results = AnalysisResultAccumulator::default();
        for source in sources {
            let result = analyze(AnalysisBatch::from_files(
                CompileOpts::default(),
                [(path.clone(), source.clone())],
            ));
            assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
            results.push(result);
        }
        let tables = results.finish().symbol_tables;
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
    let path = project.path("/Shared.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let current_target = project.read_file("/Types.sol");
    let changed_target = current_target.replace("uint256", "bytes32");

    for entries in [
        [("/left/Main.sol", &current_target), ("/right/Main.sol", &changed_target)],
        [("/right/Main.sol", &changed_target), ("/left/Main.sol", &current_target)],
    ] {
        let mut results = AnalysisResultAccumulator::default();
        let mut baseline = None;
        for (entry, target_source) in entries {
            project.write_file("/Types.sol", target_source);
            let result = analyze(AnalysisBatch::from_files(
                CompileOpts::default(),
                [(project.path(entry), project.read_file(entry))],
            ));
            assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
            baseline.get_or_insert_with(|| result.symbol_tables.clone());
            results.push(result);
        }
        let baseline = baseline.unwrap();
        let tables = results.finish().symbol_tables;
        assert_no_point_query(&tables, &uri, marked.marker("$1").position());
        for marker in ["$2", "$3"] {
            let position = marked.marker(marker).position();
            let definition = baseline.goto_definition(&uri, position);
            assert!(definition.is_some());
            assert_eq!(tables.goto_definition(&uri, position), definition);
            assert!(baseline.goto_type_definition(&uri, position).is_some());
            assert_eq!(tables.goto_type_definition(&uri, position), None);
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
    let uri = Url::from_file_path(project.path("/Shared.sol")).unwrap();
    let position = marked.marker("$1").position();

    for paths in [["/left/Main.sol", "/right/Main.sol"], ["/right/Main.sol", "/left/Main.sol"]] {
        let mut results = AnalysisResultAccumulator::default();
        let mut baseline = None;
        for path in paths {
            let result = analyze(AnalysisBatch::from_files(
                CompileOpts::default(),
                [(project.path(path), project.read_file(path))],
            ));
            // The ambiguous call deliberately produces a diagnostic and two navigation targets.
            assert!(!result.diagnostics.is_empty());
            baseline.get_or_insert_with(|| result.symbol_tables.clone());
            results.push(result);
        }
        let baseline = baseline.unwrap();
        let tables = results.finish().symbol_tables;
        let definition = baseline.goto_definition(&uri, position);
        let type_definition = baseline.goto_type_definition(&uri, position);
        for response in [&definition, &type_definition] {
            let Some(GotoDefinitionResponse::Array(locations)) = response else {
                panic!("expected ambiguous navigation targets");
            };
            assert_eq!(locations.len(), 2);
        }
        assert_eq!(tables.goto_definition(&uri, position), definition);
        assert_eq!(
            tables.goto_declaration(&uri, position),
            baseline.goto_declaration(&uri, position)
        );
        assert_eq!(tables.goto_type_definition(&uri, position), type_definition);
        assert_eq!(
            tables.document_highlights(&uri, position),
            baseline.document_highlights(&uri, position)
        );
        assert_eq!(tables.hover(&uri, position), None);
    }
}
