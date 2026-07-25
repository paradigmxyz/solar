use super::{AnalysisBatch, analyze};
use crate::test_support::MarkedProject;
use lsp_types::{Position, Range, Url};
use solar_config::CompileOpts;

#[test]
fn basic_direct_call_hierarchy() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Calls.sol
        contract C {
            function $1callee() internal {}
            function $2caller() external {
                $3callee();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Calls.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Calls.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();

    let callee =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();
    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$3").position()),
        Some(vec![callee.clone()])
    );

    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, callee);
    assert_eq!(outgoing[0].from_ranges, [marker_range(&marked, "$3", 6)]);

    let incoming = tables.call_hierarchy_incoming(&outgoing[0].to).unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from, caller);
    assert_eq!(incoming[0].from_ranges, outgoing[0].from_ranges);
}

#[test]
fn prepares_enclosing_callable_bodies_only() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Prepare.sol
        $5contract C {
            modifier $1guarded() {
                $2_;
            }

            function $3f() external {
                uint256 $4value = 1;
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Prepare.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Prepare.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();

    let modifier =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let function =
        tables.prepare_call_hierarchy(&uri, marked.marker("$3").position()).unwrap().pop().unwrap();

    assert_eq!(modifier.name, "guarded");
    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()),
        Some(vec![modifier])
    );
    assert_eq!(function.name, "f");
    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$4").position()),
        Some(vec![function])
    );
    assert_eq!(tables.prepare_call_hierarchy(&uri, marked.marker("$5").position()), None);
}

#[test]
fn groups_repeated_calls_and_preserves_recursion() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Repeated.sol
        contract C {
            function $1callee() internal {}
            function $2caller() external {
                $3callee();
                $4callee();
                $5caller();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Repeated.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Repeated.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let callee =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();

    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    let repeated = outgoing.iter().find(|call| call.to == callee).unwrap();
    assert_eq!(
        repeated.from_ranges,
        [marker_range(&marked, "$3", 6), marker_range(&marked, "$4", 6)]
    );
    let recursive = outgoing.iter().find(|call| call.to == caller).unwrap();
    assert_eq!(recursive.from_ranges, [marker_range(&marked, "$5", 6)]);

    let incoming = tables.call_hierarchy_incoming(&callee).unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from, caller);
    assert_eq!(incoming[0].from_ranges, repeated.from_ranges);
}

#[test]
fn indexes_modifier_applications_and_arguments() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Modifiers.sol
        contract C {
            function $1argument() internal pure returns (uint256) { return 1; }
            modifier $2guarded(uint256) { _; }

            function $3caller() external $4guarded($5argument()) {}
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Modifiers.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Modifiers.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let argument =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let modifier =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$3").position()).unwrap().pop().unwrap();

    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$4").position()),
        Some(vec![modifier.clone()])
    );
    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    let modifier_call = outgoing.iter().find(|call| call.to == modifier).unwrap();
    assert_eq!(modifier_call.from_ranges, [marker_range(&marked, "$4", 7)]);
    let argument_call = outgoing.iter().find(|call| call.to == argument).unwrap();
    assert_eq!(argument_call.from_ranges, [marker_range(&marked, "$5", 8)]);
}

#[test]
fn uses_exact_qualified_modifier_name_range() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /QualifiedModifier.sol
        contract Base {
            modifier $1guarded() { _; }
        }

        contract C is Base {
            function $2caller() external Base.$3guarded /* gap */ () {}
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/QualifiedModifier.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/QualifiedModifier.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let modifier =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();

    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$3").position()),
        Some(vec![modifier.clone()])
    );
    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, modifier);
    assert_eq!(outgoing[0].from_ranges, [marker_range(&marked, "$3", 7)]);
}

#[test]
fn preserves_cross_file_call_identity() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Lib.sol
        library Lib {
            function $1target(uint256) internal pure {}
        }
        //- /Caller.sol
        import "./Lib.sol";

        contract C {
            function $2caller() external {
                Lib.$3target(1);
            }
        }
        "#,
    );
    let project = marked.project();
    let lib_path = project.path("/Lib.sol");
    let caller_path = project.path("/Caller.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [
            (lib_path.clone(), project.read_file("/Lib.sol")),
            (caller_path.clone(), project.read_file("/Caller.sol")),
        ],
    ))
    .symbol_tables;
    let lib_uri = Url::from_file_path(lib_path).unwrap();
    let caller_uri = Url::from_file_path(caller_path).unwrap();
    let target = tables
        .prepare_call_hierarchy(&lib_uri, marked.marker("$1").position())
        .unwrap()
        .pop()
        .unwrap();
    let caller = tables
        .prepare_call_hierarchy(&caller_uri, marked.marker("$2").position())
        .unwrap()
        .pop()
        .unwrap();

    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, target);
    assert_eq!(outgoing[0].to.uri, lib_uri);
    assert_eq!(outgoing[0].from_ranges, [marker_range(&marked, "$3", 6)]);
    let incoming = tables.call_hierarchy_incoming(&outgoing[0].to).unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from, caller);
    assert_eq!(incoming[0].from.uri, caller_uri);
}

#[test]
fn uses_typed_targets_for_call_sites() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Typed.sol
        library Lib {
            function $1attached(uint256) internal pure {}
        }

        contract Base {
            function $2inherited(uint256) internal virtual {}
        }

        contract C is Base {
            using Lib for uint256;

            function $3overloaded(uint256) internal {}
            function $4overloaded(address) internal {}
            function $5inherited(uint256) internal override {}
            function $6externalCall() external {}

            function $7caller(uint256 value) external {
                $8overloaded(value);
                $9overloaded(address(this));
                value.$10attached();
                this.$11externalCall();
                super.$12inherited(value);
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Typed.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Typed.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let item = |marker: &str| {
        tables
            .prepare_call_hierarchy(&uri, marked.marker(marker).position())
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    };
    let attached = item("$1");
    let base_inherited = item("$2");
    let integer_overload = item("$3");
    let address_overload = item("$4");
    let override_inherited = item("$5");
    let external_call = item("$6");
    let caller = item("$7");

    assert_eq!(item("$8"), integer_overload);
    assert_eq!(item("$9"), address_overload);
    assert_eq!(item("$10"), attached);
    assert_eq!(item("$11"), external_call);
    assert_eq!(item("$12"), base_inherited);
    assert_ne!(item("$12"), override_inherited);

    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 5);
    assert!(outgoing.iter().any(|call| call.to == integer_overload));
    assert!(outgoing.iter().any(|call| call.to == address_overload));
    assert!(outgoing.iter().any(|call| call.to == attached));
    assert!(outgoing.iter().any(|call| call.to == external_call));
    assert!(outgoing.iter().any(|call| call.to == base_inherited));
    assert!(!outgoing.iter().any(|call| call.to == override_inherited));
}

#[test]
fn resolves_parenthesized_direct_calls() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Parenthesized.sol
        contract Base {
            function $1inherited() internal virtual {}
        }

        contract C is Base {
            function $2direct() internal {}
            function inherited() internal override {}

            function $3caller() external {
                ($4direct)();
                ((super).$5inherited)();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Parenthesized.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Parenthesized.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let item = |marker: &str| {
        tables
            .prepare_call_hierarchy(&uri, marked.marker(marker).position())
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    };
    let inherited = item("$1");
    let direct = item("$2");
    let caller = item("$3");

    assert_eq!(item("$4"), direct);
    assert_eq!(item("$5"), inherited);
    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.iter().any(|call| call.to == direct));
    assert!(outgoing.iter().any(|call| call.to == inherited));
}

#[test]
fn excludes_non_direct_and_non_source_calls() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Excluded.sol
        contract Created {}

        contract C {
            uint256 public value;
            event Called();
            error Failed();

            function $1target() internal {}

            function $2caller() external {
                function() internal pointer = target;
                pointer();
                require(true);
                address(this).call("");
                this.value();
                new Created();
                emit Called();
                $3target();
                assembly {
                    function yulTarget() {}
                    yulTarget()
                }
                revert Failed();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Excluded.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Excluded.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let target =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();

    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, target);
    assert_eq!(outgoing[0].from_ranges, [marker_range(&marked, "$3", 6)]);
}

#[test]
fn excludes_calls_without_typed_resolution() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Unresolved.sol
        contract C {
            function target() internal pure returns (uint256) { return 1; }

            function $1caller() external {
                require(true, target(), "extra");
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Unresolved.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Unresolved.sol"))],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();

    assert_eq!(tables.call_hierarchy_outgoing(&caller), Some(Vec::new()));
}

#[test]
fn merges_identical_analysis_contexts_without_duplicate_edges() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Merged.sol
        contract C {
            function $1callee() internal {}
            function $2caller() external {
                $3callee();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Merged.sol");
    let contents = project.read_file("/Merged.sol");
    let mut tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), contents.clone())],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(&path).unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()).unwrap().pop().unwrap();
    let duplicate = analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, contents)]))
        .symbol_tables;

    tables.extend(duplicate);

    assert_eq!(
        tables.prepare_call_hierarchy(&uri, marked.marker("$2").position()),
        Some(vec![caller.clone()])
    );
    let outgoing = tables.call_hierarchy_outgoing(&caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].from_ranges, [marker_range(&marked, "$3", 6)]);
    let incoming = tables.call_hierarchy_incoming(&outgoing[0].to).unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from, caller);
}

#[test]
fn stable_items_follow_body_only_reanalysis() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Fresh.sol
        contract C {
            function $1calleeA() internal {}
            function $2calleeB() internal {}
            function $3caller() external {
                calleeA();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Fresh.sol");
    let old_contents = project.read_file("/Fresh.sol");
    let old_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), old_contents.clone())],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(&path).unwrap();
    let old_caller = old_tables
        .prepare_call_hierarchy(&uri, marked.marker("$3").position())
        .unwrap()
        .pop()
        .unwrap();

    let new_contents = old_contents
        .replace("        calleeA();", "        uint256 value = 1;\n        calleeB();");
    let new_tables =
        analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, new_contents)]))
            .symbol_tables;
    let new_caller = new_tables
        .prepare_call_hierarchy(&uri, marked.marker("$3").position())
        .unwrap()
        .pop()
        .unwrap();
    let new_callee = new_tables
        .prepare_call_hierarchy(&uri, marked.marker("$2").position())
        .unwrap()
        .pop()
        .unwrap();

    assert_eq!(old_caller.selection_range, new_caller.selection_range);
    assert_ne!(old_caller.range, new_caller.range);
    let outgoing = new_tables.call_hierarchy_outgoing(&old_caller).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, new_callee);
}

#[test]
fn rejects_invalid_or_moved_items() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Stale.sol
        contract C {
            function callee() internal {}
            function $1caller() external {
                callee();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Stale.sol");
    let contents = project.read_file("/Stale.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), contents.clone())],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(&path).unwrap();
    let item =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();

    let mut missing_data = item.clone();
    missing_data.data = None;
    assert_eq!(tables.call_hierarchy_outgoing(&missing_data), None);
    let mut malformed_data = item.clone();
    malformed_data.data = Some(serde_json::json!({ "version": "invalid" }));
    assert_eq!(tables.call_hierarchy_outgoing(&malformed_data), None);
    let mut renamed = item.clone();
    renamed.name = "other".into();
    assert_eq!(tables.call_hierarchy_outgoing(&renamed), None);

    let moved_contents = contents.replace("    function caller", "\n    function caller");
    let moved_tables =
        analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, moved_contents)]))
            .symbol_tables;
    assert_eq!(moved_tables.call_hierarchy_outgoing(&item), None);
}

#[test]
fn isolates_conflicting_source_snapshots() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Conflict.sol
        contract C {
            function callee() internal {}
            function $1caller() external {
                callee();
            }
        }
        "#,
    );
    let project = marked.project();
    let path = project.path("/Conflict.sol");
    let contents = project.read_file("/Conflict.sol");
    let mut tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), contents.clone())],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(&path).unwrap();
    let caller =
        tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop().unwrap();
    let conflicting_contents = contents.replace("        callee();", "        uint256 value = 1;");
    let conflicting =
        analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, conflicting_contents)]))
            .symbol_tables;

    tables.extend(conflicting);

    assert_eq!(tables.prepare_call_hierarchy(&uri, marked.marker("$1").position()), None);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);
}

#[test]
fn isolates_identical_callers_with_conflicting_outgoing_facts() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Target.sol
        library Target {
            function target() internal pure {}
        }
        //- /Caller.sol
        import {Target} from "./Target.sol";

        contract C {
            function $1caller() external {
                Target.target();
            }
        }
        //- /RootA.sol
        import "./Caller.sol";
        //- /RootB.sol
        import "./Caller.sol";
        "#,
    );
    let project = marked.project();
    let target_contents = project.read_file("/Target.sol");
    let mut tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(project.path("/RootA.sol"), project.read_file("/RootA.sol"))],
    ))
    .symbol_tables;
    let caller_uri = Url::from_file_path(project.path("/Caller.sol")).unwrap();
    let caller = tables
        .prepare_call_hierarchy(&caller_uri, marked.marker("$1").position())
        .unwrap()
        .pop()
        .unwrap();
    let moved_target = target_contents.replace("    function target()", "\n    function target()");
    project.write_file("/Target.sol", &moved_target);
    let conflicting = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(project.path("/RootB.sol"), project.read_file("/RootB.sol"))],
    ))
    .symbol_tables;

    tables.extend(conflicting);

    assert_eq!(tables.prepare_call_hierarchy(&caller_uri, marked.marker("$1").position()), None);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);
}

#[test]
fn rejects_partial_outgoing_results_for_conflicting_callees() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Target.sol
        library Target {
            function target() internal pure {}
        }
        //- /Caller.sol
        import {Target} from "./Target.sol";

        contract C {
            function $1caller() external {
                Target.target();
            }
        }
        //- /RootA.sol
        import "./Caller.sol";
        //- /RootB.sol
        import "./Caller.sol";
        "#,
    );
    let project = marked.project();
    let target_contents = project.read_file("/Target.sol");
    let mut tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(project.path("/RootA.sol"), project.read_file("/RootA.sol"))],
    ))
    .symbol_tables;
    let caller_uri = Url::from_file_path(project.path("/Caller.sol")).unwrap();
    let caller = tables
        .prepare_call_hierarchy(&caller_uri, marked.marker("$1").position())
        .unwrap()
        .pop()
        .unwrap();
    let changed_target = target_contents.replace(
        "function target() internal pure {}",
        "function target() internal pure { uint256 value = 1; }",
    );
    project.write_file("/Target.sol", &changed_target);
    let conflicting = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(project.path("/RootB.sol"), project.read_file("/RootB.sol"))],
    ))
    .symbol_tables;

    tables.extend(conflicting);

    assert_eq!(
        tables.prepare_call_hierarchy(&caller_uri, marked.marker("$1").position()),
        Some(vec![caller.clone()])
    );
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);
}

fn marker_range(marked: &MarkedProject, marker: &str, utf16_len: u32) -> Range {
    let start = marked.marker(marker).position();
    Range::new(start, Position::new(start.line, start.character + utf16_len))
}
