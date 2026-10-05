use super::*;
use crate::symbols::SymbolTablesAggregator;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, SymbolKind,
};

#[test]
fn groups_direct_calls_and_selects_call_site_endpoints() {
    let calls = Calls::new(
        r#"
        //- /Calls.sol
        contract C {
            function $1callee() internal {}
            function $2caller() public {
                $3callee();
                $4callee();
                $5caller();
            }
        }
        "#,
    );
    let tables = calls.analyze(&["/Calls.sol"]);
    assert!(!tables.call_hierarchy_is_initialized());
    let callee = calls.item(&tables, "$1");
    assert!(tables.call_hierarchy_is_initialized());
    let cloned_tables = tables.clone();
    assert!(!cloned_tables.call_hierarchy_is_initialized());
    let caller = calls.item(&tables, "$2");

    assert_eq!(calls.prepare_at(&tables, "$3", 0), Some(vec![callee.clone()]));
    // Call ranges are end-exclusive, so the first position after the callee belongs to the body.
    assert_eq!(calls.prepare_at(&tables, "$3", 6), Some(vec![caller.clone()]));
    let repeated = vec![calls.range("$3", 6), calls.range("$4", 6)];
    let expected_outgoing = Some(vec![
        outgoing(&callee, repeated.clone()),
        outgoing(&caller, vec![calls.range("$5", 6)]),
    ]);
    let expected_incoming = Some(vec![incoming(&caller, repeated)]);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), expected_outgoing);
    assert_eq!(tables.call_hierarchy_incoming(&callee), expected_incoming);

    // Merging identical analysis contexts neither initializes the index nor duplicates edges.
    let duplicate = calls.analyze(&["/Calls.sol"]);
    assert!(!duplicate.call_hierarchy_is_initialized());
    let tables = merge_symbol_tables(tables, duplicate);
    assert!(!tables.call_hierarchy_is_initialized());
    assert_eq!(calls.prepare_at(&tables, "$2", 0), Some(vec![caller.clone()]));
    assert!(tables.call_hierarchy_is_initialized());
    assert_eq!(calls.item(&tables, "$1"), callee);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), expected_outgoing);
    assert_eq!(tables.call_hierarchy_incoming(&callee), expected_incoming);
}

#[test]
fn indexes_modifier_applications_and_arguments() {
    let calls = Calls::new(
        r#"
        //- /Modifiers.sol
        $8contract Base {
            modifier $6baseGuard() { _; }
        }

        contract C is Base {
            function $1argument() internal pure returns (uint256) { return 1; }
            modifier $2guarded(uint256) { $9_; }

            function $3caller() external $4guarded($5argument()) Base.$7baseGuard /* gap */ () {
                uint256 $10value = 1;
            }
        }
        "#,
    );
    let tables = calls.analyze(&["/Modifiers.sol"]);
    let argument = calls.item(&tables, "$1");
    let modifier = calls.item(&tables, "$2");
    let base_modifier = calls.item(&tables, "$6");
    let caller = calls.item(&tables, "$3");

    // Only enclosing callable bodies prepare an item.
    assert_eq!((modifier.name.as_str(), caller.name.as_str()), ("guarded", "caller"));
    assert_eq!(calls.prepare_at(&tables, "$9", 0), Some(vec![modifier.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$10", 0), Some(vec![caller.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$8", 0), None);

    assert_eq!(calls.prepare_at(&tables, "$4", 0), Some(vec![modifier.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$7", 0), Some(vec![base_modifier.clone()]));
    assert_eq!(
        tables.call_hierarchy_outgoing(&caller),
        Some(vec![
            outgoing(&base_modifier, vec![calls.range("$7", 9)]),
            outgoing(&argument, vec![calls.range("$5", 8)]),
            outgoing(&modifier, vec![calls.range("$4", 7)]),
        ])
    );
}

#[test]
fn preserves_cross_file_call_identity() {
    let calls = Calls::new(
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
    let tables = calls.analyze(&["/Lib.sol", "/Caller.sol"]);
    let target = calls.item(&tables, "$1");
    let caller = calls.item(&tables, "$2");

    assert_eq!(target.uri, calls.uri("$1"));
    assert_eq!(caller.uri, calls.uri("$2"));
    let ranges = vec![calls.range("$3", 6)];
    assert_eq!(
        tables.call_hierarchy_outgoing(&caller),
        Some(vec![outgoing(&target, ranges.clone())])
    );
    assert_eq!(tables.call_hierarchy_incoming(&target), Some(vec![incoming(&caller, ranges)]));
}

#[test]
fn uses_typed_targets_for_call_sites() {
    let calls = Calls::new(
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
            function $15direct() internal {}

            function $7caller(uint256 value) external {
                $8overloaded(value);
                $9overloaded(address(this));
                value.$10attached();
                this.$11externalCall();
                super.$12inherited(value);
                ($13direct)();
                ((super).$14inherited)(value);
            }
        }
        "#,
    );
    let tables = calls.analyze(&["/Typed.sol"]);
    let item = |marker| calls.item(&tables, marker);

    assert_eq!(item("$8"), item("$3"));
    assert_eq!(item("$9"), item("$4"));
    assert_eq!(item("$10"), item("$1"));
    assert_eq!(item("$11"), item("$6"));
    assert_eq!(item("$12"), item("$2"));
    assert_ne!(item("$12"), item("$5"));
    assert_eq!(item("$13"), item("$15"));
    assert_eq!(item("$14"), item("$2"));
    let targets = tables
        .call_hierarchy_outgoing(&item("$7"))
        .unwrap()
        .into_iter()
        .map(|call| call.to)
        .collect::<Vec<_>>();
    assert_eq!(targets, ["$1", "$2", "$3", "$4", "$6", "$15"].map(item));
}

#[test]
fn indexes_constructor_invocations_and_arguments() {
    let calls = Calls::new(
        r#"
        //- /Constructors.sol
        function $1argument() pure returns (uint256) { return 1; }

        contract Target {
            $2constructor(uint256 value) {}
        }

        contract Base {
            $3constructor(uint256 value) {}
        }

        contract Derived is Base {
            $4constructor() $5Base(1) {}
        }

        contract Listed is $6Base($7argument()) {
            $8constructor() {}
        }

        contract C {
            function $9deploy() external {
                new $10Target(1);
            }
        }
        "#,
    );
    let tables = calls.analyze(&["/Constructors.sol"]);
    let [argument, target, base, derived, listed, deploy] =
        ["$1", "$2", "$3", "$4", "$8", "$9"].map(|marker| calls.item(&tables, marker));

    assert_eq!(calls.prepare_at(&tables, "$10", 0), Some(vec![target.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$5", 0), Some(vec![base.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$6", 0), Some(vec![base.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$7", 0), Some(vec![argument.clone()]));
    let creation = vec![calls.range("$10", 6)];
    assert_eq!(
        tables.call_hierarchy_outgoing(&deploy),
        Some(vec![outgoing(&target, creation.clone())])
    );
    assert_eq!(tables.call_hierarchy_incoming(&target), Some(vec![incoming(&deploy, creation)]));
    assert_eq!(
        tables.call_hierarchy_outgoing(&derived),
        Some(vec![outgoing(&base, vec![calls.range("$5", 4)])])
    );
    assert_eq!(
        tables.call_hierarchy_outgoing(&listed),
        Some(vec![
            outgoing(&argument, vec![calls.range("$7", 8)]),
            outgoing(&base, vec![calls.range("$6", 4)]),
        ])
    );
    assert_eq!(
        tables.call_hierarchy_incoming(&base),
        Some(vec![
            incoming(&derived, vec![calls.range("$5", 4)]),
            incoming(&listed, vec![calls.range("$6", 4)]),
        ])
    );
}

#[test]
fn excludes_non_direct_and_non_source_calls() {
    let calls = Calls::new(
        r#"
        //- /Excluded.sol
        abstract contract AbstractCreated {
            constructor() {}
        }

        contract C {
            uint256 public value;
            event Called();
            error Failed();

            function $1target() internal pure returns (uint256) { return 1; }

            function $2caller() external {
                function() internal pure returns (uint256) pointer = target;
                pointer();
                require(true);
                address(this).call("");
                this.value();
                new AbstractCreated();
                emit Called();
                $3target();
                assembly {
                    function yulTarget() {}
                    yulTarget()
                }
                revert Failed();
            }

            // Calls without a typed resolution are excluded too.
            function $4unresolved() external pure {
                require(true, target(), "extra");
            }
        }
        "#,
    );
    let tables = calls.analyze(&["/Excluded.sol"]);

    assert_eq!(
        tables.call_hierarchy_outgoing(&calls.item(&tables, "$2")),
        Some(vec![outgoing(&calls.item(&tables, "$1"), vec![calls.range("$3", 6)])])
    );
    assert_eq!(tables.call_hierarchy_outgoing(&calls.item(&tables, "$4")), Some(vec![]));
}

#[test]
fn orders_call_neighbors_by_uri_and_range_after_merging() {
    let calls = Calls::new(
        r#"
        //- /Z.sol
        import {Target} from "./Target.sol";
        library Z {
            function $1z() internal { Target.$2target(); }
            function $3a() internal { Target.$4target(); }
        }
        //- /A.sol
        import {Target} from "./Target.sol";
        library A {
            function $5z() internal { Target.$6target(); Target.$7target(); }
            function $8a() internal { Target.$9target(); }
        }
        //- /Target.sol
        library Target {
            function $10target() internal {}
        }
        //- /Caller.sol
        import {Z} from "./Z.sol";
        import {A} from "./A.sol";
        contract Caller {
            function $11caller() external {
                Z.$12a();
                A.$13a();
                Z.$14z();
                A.$15z(); A.$16z();
            }
        }
        //- /RootA.sol
        import "./Caller.sol";
        //- /RootB.sol
        import "./A.sol";
        import "./Caller.sol";
        "#,
    );
    let tables = calls.analyze(&["/RootA.sol"]);
    let caller = calls.item(&tables, "$11");
    let target = calls.item(&tables, "$10");
    // URI order takes precedence over analysis order; source position takes precedence over name.
    let neighbors = ["$5", "$8", "$1", "$3"].map(|marker| calls.item(&tables, marker));
    let ranges = |markers: &[(&str, u32)]| {
        markers.iter().map(|&(marker, len)| calls.range(marker, len)).collect::<Vec<_>>()
    };
    let expected_outgoing = neighbors
        .iter()
        .zip([&[("$15", 1), ("$16", 1)][..], &[("$13", 1)], &[("$14", 1)], &[("$12", 1)]])
        .map(|(to, markers)| outgoing(to, ranges(markers)))
        .collect::<Vec<_>>();
    let expected_incoming = neighbors
        .iter()
        .zip([&[("$6", 6), ("$7", 6)][..], &[("$9", 6)], &[("$2", 6)], &[("$4", 6)]])
        .map(|(from, markers)| incoming(from, ranges(markers)))
        .collect::<Vec<_>>();
    assert_eq!(tables.call_hierarchy_outgoing(&caller), Some(expected_outgoing.clone()));
    assert_eq!(tables.call_hierarchy_incoming(&target), Some(expected_incoming.clone()));

    let tables = merge_symbol_tables(tables, calls.analyze(&["/RootB.sol"]));

    // Reusing items from before the merge must retain the complete, deduplicated response.
    assert_eq!(tables.call_hierarchy_outgoing(&caller), Some(expected_outgoing));
    assert_eq!(tables.call_hierarchy_incoming(&target), Some(expected_incoming));
}

#[test]
fn echoed_items_resolve_by_identity_across_reanalysis() {
    let calls = Calls::new(
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
    let old_tables = calls.analyze(&["/Fresh.sol"]);
    let old_caller = calls.item(&old_tables, "$3");
    let old_callee = calls.item(&old_tables, "$2");

    let mut missing_data = old_caller.clone();
    missing_data.data = None;
    let mut malformed_data = old_caller.clone();
    malformed_data.data = Some(serde_json::json!({ "version": "invalid" }));
    let mut renamed = old_caller.clone();
    renamed.name = "other".into();
    for item in [missing_data, malformed_data, renamed] {
        assert_eq!(old_tables.call_hierarchy_outgoing(&item), None);
    }

    let contents = calls.0.project().read_file("/Fresh.sol");
    let new_tables = calls.analyze_contents(
        "/Fresh.sol",
        contents
            .replace("contract C", "contract D")
            .replace("        calleeA();", "        uint256 value = 1;\n        calleeB();"),
    );
    let new_caller = calls.item(&new_tables, "$3");
    let new_callee = calls.item(&new_tables, "$2");

    assert_eq!(old_caller.selection_range, new_caller.selection_range);
    assert_ne!(old_caller.range, new_caller.range);
    assert_eq!(old_caller.detail.as_deref(), Some("C"));
    assert_eq!(new_caller.detail.as_deref(), Some("D"));
    let outgoing = new_tables.call_hierarchy_outgoing(&old_caller).unwrap();
    assert_eq!(outgoing.into_iter().map(|call| call.to).collect::<Vec<_>>(), [new_callee]);
    let incoming = new_tables.call_hierarchy_incoming(&old_callee).unwrap();
    assert_eq!(incoming.into_iter().map(|call| call.from).collect::<Vec<_>>(), [new_caller]);

    let moved = calls.analyze_contents(
        "/Fresh.sol",
        contents.replace("    function caller", "\n    function caller"),
    );
    assert_eq!(moved.call_hierarchy_outgoing(&old_caller), None);
}

#[test]
fn conflicting_batches_reject_partial_relations() {
    const FIXTURE: &str = r#"
        //- /Target.sol
        library Target {
            function $1target() internal pure {}
        }
        //- /Caller.sol
        import {Target} from "./Target.sol";

        contract C {
            function $2caller() external {
                $3uint256 value = 1;
                Target.$4target();
            }
        }
        //- /RootA.sol
        import "./Caller.sol";
        //- /RootB.sol
        import "./Caller.sol";
        "#;
    let merge_after_edit = |path, from, to| {
        let calls = Calls::new(FIXTURE);
        let first = calls.analyze(&["/RootA.sol"]);
        let (target, caller) = (calls.item(&first, "$1"), calls.item(&first, "$2"));
        let contents = calls.0.project().read_file(path).replace(from, to);
        calls.0.project().write_file(path, &contents);
        let merged = merge_symbol_tables(first, calls.analyze(&["/RootB.sol"]));
        (calls, merged, target, caller)
    };

    // A caller whose only callee moved has identical source but conflicting outgoing facts.
    let (calls, tables, _, caller) =
        merge_after_edit("/Target.sol", "    function target()", "\n    function target()");
    assert_eq!(calls.prepare_at(&tables, "$2", 0), None);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);

    // A conflicting caller must not leave a partial incoming list for its callee.
    let (calls, tables, target, caller) =
        merge_after_edit("/Caller.sol", "        Target.target();", "\n        Target.target();");
    assert_eq!(calls.prepare_at(&tables, "$2", 0), None);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);
    assert_eq!(tables.call_hierarchy_incoming(&target), None);

    // A conflicting callee must not leave a partial outgoing list for its caller.
    let (calls, tables, _, caller) =
        merge_after_edit("/Target.sol", "pure {}", "pure { uint256 value = 1; }");
    assert_eq!(calls.prepare_at(&tables, "$2", 0), Some(vec![caller.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$3", 0), Some(vec![caller.clone()]));
    assert_eq!(calls.prepare_at(&tables, "$4", 0), None);
    assert_eq!(tables.call_hierarchy_outgoing(&caller), None);
}

#[test]
fn indexes_calls_to_implicit_constructors() {
    let calls = Calls::new(
        r#"
        //- /Implicit.sol
        contract $1Empty {}
        contract $2Derived is $3Empty() {}
        contract Explicit is $10Empty {
            $4constructor() $5Empty() {}
        }
        contract Factory {
            function $6deploy() external returns (Derived) {
                return new $7Derived();
            }
        }
        interface $8I {}
        library $9Lib {}
        "#,
    );
    let tables = calls.analyze_clean(&["/Implicit.sol"]);
    let [empty, derived, explicit, deploy] =
        ["$1", "$2", "$4", "$6"].map(|m| calls.item(&tables, m));
    for (site, callee) in [("$3", &empty), ("$5", &empty), ("$7", &derived)] {
        assert_eq!(calls.prepare_at(&tables, site, 0), Some(vec![callee.clone()]));
    }
    let [to_empty, from_explicit, creation] =
        [("$3", 5), ("$5", 5), ("$7", 7)].map(|(marker, len)| vec![calls.range(marker, len)]);
    assert_eq!(
        tables.call_hierarchy_outgoing(&derived),
        Some(vec![outgoing(&empty, to_empty.clone())])
    );
    assert_eq!(
        tables.call_hierarchy_outgoing(&explicit),
        Some(vec![outgoing(&empty, from_explicit.clone())])
    );
    assert_eq!(
        tables.call_hierarchy_outgoing(&deploy),
        Some(vec![outgoing(&derived, creation.clone())])
    );
    assert_eq!(
        tables.call_hierarchy_incoming(&empty),
        Some(vec![incoming(&derived, to_empty), incoming(&explicit, from_explicit)])
    );
    assert_eq!(tables.call_hierarchy_incoming(&derived), Some(vec![incoming(&deploy, creation)]));
    // Interfaces, libraries, and bare inheritance have no constructor call.
    for marker in ["$8", "$9", "$10"] {
        assert_eq!(calls.prepare_at(&tables, marker, 0), None);
    }
}

#[test]
fn indexes_implicit_constructor_inheritance_calls() {
    let calls = Calls::new(
        r#"
        //- /Token.sol
        contract ERC20 {
            $1constructor(string memory name_, string memory symbol_) {}
        }
        contract $2T is $3ERC20("N", "S") {
            $4uint256 value;
        }
        "#,
    );
    let tables = calls.analyze_clean(&["/Token.sol"]);
    let [base, derived] = ["$1", "$2"].map(|marker| calls.item(&tables, marker));
    assert_eq!(calls.prepare_at(&tables, "$3", 0), Some(vec![base.clone()]));
    assert_eq!(
        (derived.name.as_str(), derived.kind, derived.detail.as_deref()),
        ("T", SymbolKind::CONSTRUCTOR, Some("implicit constructor"))
    );
    let call = vec![calls.range("$3", 5)];
    assert_eq!(derived.selection_range, calls.range("$2", 1));
    assert!(derived.range.start <= derived.selection_range.start);
    assert!(derived.range.end >= call[0].end);
    assert_eq!(tables.call_hierarchy_outgoing(&derived), Some(vec![outgoing(&base, call.clone())]));
    assert_eq!(tables.call_hierarchy_incoming(&base), Some(vec![incoming(&derived, call)]));
    // The whole contract must not become an enclosing constructor body.
    assert_eq!(calls.prepare_at(&tables, "$4", 0), None);
}

#[test]
fn merges_implicit_constructors_and_isolates_conflicting_base_calls() {
    let calls = Calls::new(
        r#"
        //- /Base.sol
        contract Base {
            $1constructor(uint256) {}
        }
        //- /Caller.sol
        import {Base} from "./Base.sol";
        function $2argument() pure returns (uint256) { return 1; }
        contract $3T is $4Base($5argument()) {}
        contract Factory {
            function $6deploy() external { new $7T(); }
        }
        //- /RootA.sol
        import "./Caller.sol";
        //- /RootB.sol
        import "./Caller.sol";
        //- /RootC.sol
        import "./Caller.sol";
        "#,
    );
    let tables = calls.analyze_clean(&["/RootA.sol"]);
    let [base, argument, constructor, deploy] =
        ["$1", "$2", "$3", "$6"].map(|marker| calls.item(&tables, marker));
    let [base_call, argument_call, creation] =
        [("$4", 4), ("$5", 8), ("$7", 1)].map(|(marker, len)| vec![calls.range(marker, len)]);

    // Items prepared before the merge retain one edge per source call after deduplication.
    let tables = merge_symbol_tables(tables, calls.analyze_clean(&["/RootB.sol"]));
    assert_eq!(calls.prepare_at(&tables, "$3", 0), Some(vec![constructor.clone()]));
    assert_eq!(
        tables.call_hierarchy_outgoing(&constructor),
        Some(vec![outgoing(&base, base_call.clone()), outgoing(&argument, argument_call.clone())])
    );
    assert_eq!(
        tables.call_hierarchy_incoming(&base),
        Some(vec![incoming(&constructor, base_call)])
    );
    assert_eq!(
        tables.call_hierarchy_incoming(&argument),
        Some(vec![incoming(&constructor, argument_call)])
    );
    assert_eq!(
        tables.call_hierarchy_incoming(&constructor),
        Some(vec![incoming(&deploy, creation.clone())])
    );
    assert_eq!(
        tables.call_hierarchy_outgoing(&deploy),
        Some(vec![outgoing(&constructor, creation)])
    );

    // The caller's text stays identical, but a moved imported constructor changes its call facts.
    let project = calls.0.project();
    project.write_file(
        "/Base.sol",
        &project.read_file("/Base.sol").replace("constructor(", "\nconstructor("),
    );
    let tables = merge_symbol_tables(tables, calls.analyze_clean(&["/RootC.sol"]));
    assert_eq!(calls.prepare_at(&tables, "$3", 0), None);
    assert_eq!(tables.call_hierarchy_outgoing(&constructor), None);
    assert_eq!(tables.call_hierarchy_incoming(&argument), None);
    assert_eq!(tables.call_hierarchy_outgoing(&deploy), None);
}

#[test]
fn rejects_renamed_or_explicitly_replaced_implicit_constructors() {
    let calls = Calls::new(
        r#"
        //- /Replaced.sol
        contract Base { $1constructor(uint256) {} }
        contract $2T is $3Base(1) {}
        "#,
    );
    let contents = calls.0.project().read_file("/Replaced.sol");
    let analyze = |contents: String| {
        let result = analyze_source(calls.0.project().path("/Replaced.sol"), contents);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result.symbol_tables
    };
    let tables = analyze(contents.clone());
    let [base, constructor] = ["$1", "$2"].map(|marker| calls.item(&tables, marker));

    let renamed = analyze(contents.replace("contract T", "contract U"));
    let renamed_constructor = calls.item(&renamed, "$2");
    assert_eq!(constructor.selection_range, renamed_constructor.selection_range);
    assert_eq!(renamed_constructor.name, "U");
    assert_eq!(
        renamed.call_hierarchy_outgoing(&renamed_constructor),
        Some(vec![outgoing(&base, vec![calls.range("$3", 4)])])
    );
    let explicit = analyze(
        contents.replace("contract T is Base(1) {}", "contract T is Base(1) { constructor() {} }"),
    );
    assert_eq!(calls.prepare_at(&explicit, "$2", 0), None);
    for tables in [&renamed, &explicit] {
        assert_eq!(tables.call_hierarchy_outgoing(&constructor), None);
        assert_eq!(tables.call_hierarchy_incoming(&constructor), None);
    }
}

struct Calls(MarkedProject);

impl Calls {
    fn new(fixture: &str) -> Self {
        Self(MarkedProject::from_fixture(fixture))
    }

    fn analysis(&self, paths: &[&str]) -> AnalysisResult {
        let project = self.0.project();
        analyze(AnalysisBatch::from_files(
            CompileOpts::default(),
            paths.iter().map(|path| (project.path(path), project.read_file(path))),
        ))
    }

    fn analyze(&self, paths: &[&str]) -> SymbolTables {
        self.analysis(paths).symbol_tables
    }

    fn analyze_clean(&self, paths: &[&str]) -> SymbolTables {
        let result = self.analysis(paths);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result.symbol_tables
    }

    fn analyze_contents(&self, path: &str, contents: String) -> SymbolTables {
        analyze_source(self.0.project().path(path), contents).symbol_tables
    }

    fn uri(&self, marker: &str) -> Url {
        self.0.project().uri(self.0.marker(marker).path())
    }

    fn prepare_at(
        &self,
        tables: &SymbolTables,
        marker: &str,
        offset: u32,
    ) -> Option<Vec<CallHierarchyItem>> {
        let range = self.range(marker, offset);
        tables.prepare_call_hierarchy(&self.uri(marker), range.end)
    }

    fn item(&self, tables: &SymbolTables, marker: &str) -> CallHierarchyItem {
        let [item] = self.prepare_at(tables, marker, 0).unwrap().try_into().unwrap();
        item
    }

    fn range(&self, marker: &str, utf16_len: u32) -> Range {
        let start = self.0.marker(marker).position();
        Range::new(start, Position::new(start.line, start.character + utf16_len))
    }
}

fn outgoing(to: &CallHierarchyItem, from_ranges: Vec<Range>) -> CallHierarchyOutgoingCall {
    CallHierarchyOutgoingCall { to: to.clone(), from_ranges }
}

fn incoming(from: &CallHierarchyItem, from_ranges: Vec<Range>) -> CallHierarchyIncomingCall {
    CallHierarchyIncomingCall { from: from.clone(), from_ranges }
}

pub(super) fn merge_symbol_tables(first: SymbolTables, second: SymbolTables) -> SymbolTables {
    let mut aggregator = SymbolTablesAggregator::default();
    aggregator.push(first);
    aggregator.push(second);
    aggregator.finish()
}
