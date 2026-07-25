use super::{
    AnalysisBatch, AnalysisResultAccumulator, GlobalState, SymbolTables, analyze,
    support::RequestFixture,
};
use crate::test_support::{
    TestProject, type_hierarchy_prepare_params, type_hierarchy_subtypes_params,
    type_hierarchy_supertypes_params,
};
use async_lsp::ClientSocket;
use lsp_types::{Position, Range, SymbolKind, SymbolTag, TypeHierarchyItem, Url};
use serde_json::json;
use solar_config::{CompileOpts, ImportRemapping};
use std::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};

#[test]
fn prepares_contracts_and_returns_direct_edges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hierarchy.sol
        contract $1Base {}
        contract $2Child is $3Base {}
        "#,
        "/Hierarchy.sol",
    );

    let base = fixture.prepare_type_hierarchy("$1").unwrap();
    let child = fixture.prepare_type_hierarchy("$2").unwrap();
    let base_reference = fixture.prepare_type_hierarchy("$3").unwrap();
    assert_eq!(base.len(), 1);
    assert_eq!(child.len(), 1);
    assert_eq!(base_reference, base);
    assert_eq!(base[0].name, "Base");
    assert_eq!(child[0].name, "Child");

    assert_eq!(fixture.type_hierarchy_supertypes(child[0].clone()), Some(base.clone()));
    assert_eq!(fixture.type_hierarchy_subtypes(base[0].clone()), Some(child.clone()));
    assert_eq!(fixture.type_hierarchy_supertypes(base[0].clone()), Some(Vec::new()));
    assert_eq!(fixture.type_hierarchy_subtypes(child[0].clone()), Some(Vec::new()));
}

#[test]
fn contract_edges_are_direct_in_diamonds_and_multilevel_hierarchies() {
    let fixture = RequestFixture::new(
        r#"
        //- /Diamond.sol
        contract $1Root {}
        contract $2Left is Root {}
        contract $3Right is Root {}
        contract $4Leaf is Left, Right {}
        "#,
        "/Diamond.sol",
    );
    let root = prepared(&fixture, "$1");
    let left = prepared(&fixture, "$2");
    let right = prepared(&fixture, "$3");
    let leaf = prepared(&fixture, "$4");

    assert!(names(fixture.type_hierarchy_supertypes(root.clone())).is_empty());
    assert_eq!(names(fixture.type_hierarchy_supertypes(left.clone())), ["Root"]);
    assert_eq!(names(fixture.type_hierarchy_supertypes(right.clone())), ["Root"]);
    assert_eq!(names(fixture.type_hierarchy_supertypes(leaf.clone())), ["Left", "Right"]);
    assert_eq!(names(fixture.type_hierarchy_subtypes(root)), ["Left", "Right"]);
    assert_eq!(names(fixture.type_hierarchy_subtypes(left)), ["Leaf"]);
    assert_eq!(names(fixture.type_hierarchy_subtypes(right)), ["Leaf"]);
    assert!(names(fixture.type_hierarchy_subtypes(leaf)).is_empty());
}

#[test]
fn presents_all_supported_declarations_and_callable_edges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Callables.sol
        function $1freeFunction(uint256 value) pure returns (uint256) { return value; }
        function callFreeFunction() pure returns (uint256) { return $16freeFunction(1); }
        interface $2Iface {}
        library $3Lib {}

        abstract contract $4Base {
            function $5value() external view virtual returns (uint256);
            function $6run(uint256 value) public virtual returns (uint256) { return value; }
            modifier $7guard() virtual { _; }
        }

        contract $8Derived is Base {
            uint256 public override $9value;

            $10constructor(uint256 initial) { value = initial; }
            $11fallback() external {}
            $12receive() external payable {}

            function $13run(uint256 value_) public pure override returns (uint256) {
                return value_;
            }

            modifier $14guard() override { _; }

            function protected() public $17guard {}
            function $18hidden() private {}

            function read() external view returns (uint256) {
                return this.$15value();
            }
        }
        "#,
        "/Callables.sol",
    );

    assert_item(&prepared(&fixture, "$1"), "freeFunction(uint256)", SymbolKind::FUNCTION);
    assert_item(&prepared(&fixture, "$2"), "Iface", SymbolKind::INTERFACE);
    assert_item(&prepared(&fixture, "$3"), "Lib", SymbolKind::MODULE);
    assert_item(&prepared(&fixture, "$4"), "Base", SymbolKind::CLASS);
    assert_item(&prepared(&fixture, "$5"), "Base.value()", SymbolKind::METHOD);
    assert_item(&prepared(&fixture, "$6"), "Base.run(uint256)", SymbolKind::METHOD);
    assert_item(&prepared(&fixture, "$7"), "Base.guard", SymbolKind::FUNCTION);
    assert_item(&prepared(&fixture, "$8"), "Derived", SymbolKind::CLASS);
    assert_item(&prepared(&fixture, "$9"), "Derived.value", SymbolKind::PROPERTY);
    assert_item(
        &prepared(&fixture, "$10"),
        "Derived.constructor(uint256)",
        SymbolKind::CONSTRUCTOR,
    );
    assert_item(&prepared(&fixture, "$11"), "Derived.fallback()", SymbolKind::FUNCTION);
    assert_item(&prepared(&fixture, "$12"), "Derived.receive()", SymbolKind::FUNCTION);
    assert_item(&prepared(&fixture, "$13"), "Derived.run(uint256)", SymbolKind::METHOD);
    assert_item(&prepared(&fixture, "$14"), "Derived.guard", SymbolKind::FUNCTION);
    assert_eq!(prepared(&fixture, "$15"), prepared(&fixture, "$9"));
    assert_eq!(prepared(&fixture, "$16"), prepared(&fixture, "$1"));
    assert_eq!(prepared(&fixture, "$17"), prepared(&fixture, "$14"));
    assert_item(&prepared(&fixture, "$18"), "Derived.hidden()", SymbolKind::METHOD);

    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$9"))),
        ["Base.value()"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$13"))),
        ["Base.run(uint256)"]
    );
    assert_eq!(names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$14"))), ["Base.guard"]);
    assert_eq!(names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$5"))), ["Derived.value"]);
}

#[test]
fn indexes_implicit_interface_getter_overrides() {
    let fixture = RequestFixture::new(
        r#"
        //- /Getter.sol
        interface Interface {
            function $1value() external view returns (uint256);
        }

        contract Implementation is Interface {
            uint256 public $2value;
        }
        "#,
        "/Getter.sol",
    );

    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$2"))),
        ["Interface.value()"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$1"))),
        ["Implementation.value"]
    );
}

#[test]
fn special_function_override_edges_are_direct() {
    let fixture = RequestFixture::new(
        r#"
        //- /Special.sol
        abstract contract Base {
            $1fallback() external virtual {}
            $2receive() external payable virtual {}
        }

        contract Child is Base {
            $3fallback() external override {}
            $4receive() external payable override {}
        }
        "#,
        "/Special.sol",
    );

    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$3"))),
        ["Base.fallback()"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$4"))),
        ["Base.receive()"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$1"))),
        ["Child.fallback()"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$2"))),
        ["Child.receive()"]
    );
}

#[test]
fn uses_exact_declaration_and_name_or_keyword_ranges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Ranges.sol
        contract $1C {
            $2constructor() {}
            $3fallback() external {}
            $4receive() external payable {}
        }
        "#,
        "/Ranges.sol",
    );

    let contract = prepared(&fixture, "$1");
    assert_eq!(contract.range, Range::new(Position::new(0, 0), Position::new(4, 1)));
    assert_eq!(contract.selection_range, Range::new(Position::new(0, 9), Position::new(0, 10)));

    let constructor = prepared(&fixture, "$2");
    assert_eq!(constructor.range, Range::new(Position::new(1, 4), Position::new(1, 20)));
    assert_eq!(constructor.selection_range, Range::new(Position::new(1, 4), Position::new(1, 15)));

    let fallback = prepared(&fixture, "$3");
    assert_eq!(fallback.range, Range::new(Position::new(2, 4), Position::new(2, 26)));
    assert_eq!(fallback.selection_range, Range::new(Position::new(2, 4), Position::new(2, 12)));

    let receive = prepared(&fixture, "$4");
    assert_eq!(receive.range, Range::new(Position::new(3, 4), Position::new(3, 33)));
    assert_eq!(receive.selection_range, Range::new(Position::new(3, 4), Position::new(3, 11)));
}

#[test]
fn rejects_unsupported_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Unsupported.sol
        type $1Value is uint256;

        contract C {
            uint256 public $2plain;
            uint256 private $3hidden;
            struct $4Data { uint256 value; }
            enum $5Choice { A }
            event $6Changed(uint256 value);
            error $7Failed();

            function use(uint256 $8parameter) public pure {
                uint256 $9local = parameter;
                assembly {
                    function $10yulFunction() -> result { result := 1 }
                    pop(yulFunction())
                }
            }
        }
        "#,
        "/Unsupported.sol",
    );

    for marker in ["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10"] {
        assert_eq!(fixture.prepare_type_hierarchy(marker), None, "marker {marker}");
    }
}

#[test]
fn validates_the_full_echoed_item_and_opaque_data() {
    let fixture = RequestFixture::new(
        r#"
        //- /Validation.sol
        contract $1Base {}
        contract Child is Base {}
        "#,
        "/Validation.sol",
    );
    let item = prepared(&fixture, "$1");
    let data = item.data.as_ref().unwrap().as_object().unwrap();
    assert_eq!(data.len(), 3);
    assert_eq!(data["version"], 1);
    assert_eq!(data["uri"], json!(item.uri));
    assert_eq!(data["selectionRange"], json!(item.selection_range));

    let mut tampered = Vec::new();
    let mut changed = item.clone();
    changed.name.push_str("Changed");
    tampered.push(changed);
    let mut changed = item.clone();
    changed.kind = SymbolKind::INTERFACE;
    tampered.push(changed);
    let mut changed = item.clone();
    changed.tags = Some(SymbolTag::DEPRECATED);
    tampered.push(changed);
    let mut changed = item.clone();
    changed.detail = Some("changed".into());
    tampered.push(changed);
    let mut changed = item.clone();
    changed.uri = Url::from_file_path(std::env::temp_dir().join("Other.sol")).unwrap();
    tampered.push(changed);
    let mut changed = item.clone();
    changed.range.end.character += 1;
    tampered.push(changed);
    let mut changed = item.clone();
    changed.selection_range.end.character += 1;
    tampered.push(changed);

    for data in [
        None,
        Some(json!(null)),
        Some(json!({})),
        Some(json!({
            "version": 2,
            "uri": item.uri,
            "selectionRange": item.selection_range,
        })),
        Some(json!({
            "version": 1,
            "uri": Url::from_file_path(std::env::temp_dir().join("Other.sol")).unwrap(),
            "selectionRange": item.selection_range,
        })),
        Some(json!({
            "version": 1,
            "uri": item.uri,
            "selectionRange": Range::new(Position::new(9, 0), Position::new(9, 1)),
        })),
        Some(json!({
            "version": 1,
            "uri": item.uri,
            "selectionRange": item.selection_range,
            "extra": true,
        })),
    ] {
        let mut changed = item.clone();
        changed.data = data;
        tampered.push(changed);
    }

    for changed in tampered {
        assert_eq!(fixture.type_hierarchy_supertypes(changed.clone()), None);
        assert_eq!(fixture.type_hierarchy_subtypes(changed), None);
    }
}

#[test]
fn callable_edges_are_direct_and_keep_overloads_separate() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overrides.sol
        abstract contract Base {
            function $1run(uint256 value) public virtual {}
            function $2run(address value) public virtual {}
        }

        abstract contract Middle is Base {
            function $3run(uint256 value) public virtual override {}
            function $4run(address value) public virtual override {}
        }

        contract Leaf is Middle {
            function $5run(uint256 value) public override {}
            function $6run(address value) public override {}

            function use() public {
                $7run(1);
                $8run(address(0));
            }
        }
        "#,
        "/Overrides.sol",
    );

    assert_eq!(prepared(&fixture, "$7"), prepared(&fixture, "$5"));
    assert_eq!(prepared(&fixture, "$8"), prepared(&fixture, "$6"));
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$1"))),
        ["Middle.run(uint256)"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$2"))),
        ["Middle.run(address)"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$5"))),
        ["Middle.run(uint256)"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_supertypes(prepared(&fixture, "$6"))),
        ["Middle.run(address)"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$3"))),
        ["Leaf.run(uint256)"]
    );
    assert_eq!(
        names(fixture.type_hierarchy_subtypes(prepared(&fixture, "$4"))),
        ["Leaf.run(address)"]
    );
}

#[test]
fn canonical_names_keep_user_defined_parameter_types_distinct() {
    let fixture = RequestFixture::new(
        r#"
        //- /Names.sol
        type Amount is uint256;
        contract First {}
        contract Second {}

        contract C {
            struct Data { uint256 value; }
            enum Choice { A }

            function $1choose(First value) internal {}
            function $2choose(Second value) internal {}
            function $3inspect(Data memory value) internal {}
            function $4inspect(Choice value) internal {}
            function $5inspect(Amount value) internal {}
        }
        "#,
        "/Names.sol",
    );

    assert_eq!(prepared(&fixture, "$1").name, "C.choose(contract First)");
    assert_eq!(prepared(&fixture, "$2").name, "C.choose(contract Second)");
    assert_eq!(prepared(&fixture, "$3").name, "C.inspect(struct C.Data)");
    assert_eq!(prepared(&fixture, "$4").name, "C.inspect(enum C.Choice)");
    assert_eq!(prepared(&fixture, "$5").name, "C.inspect(Amount)");
}

#[test]
fn merges_identical_cross_batch_nodes_and_edges_in_both_orders() {
    let source = r#"
        //- /Shared.sol
        contract $1Base {}

        //- /first/Main.sol
        import "../Shared.sol";
        contract $2Zed is Base {}

        //- /second/Main.sol
        import "../Shared.sol";
        contract $3Alpha is Base {}
        "#;

    for paths in [["/first/Main.sol", "/second/Main.sol"], ["/second/Main.sol", "/first/Main.sol"]]
    {
        let fixture = RequestFixture::new_in_batches(source, &paths);
        let base = prepared(&fixture, "$1");
        let zed = prepared(&fixture, "$2");
        let alpha = prepared(&fixture, "$3");

        assert_eq!(
            names(fixture.type_hierarchy_subtypes(base)),
            ["Zed", "Alpha"],
            "batch order {paths:?}"
        );
        assert_eq!(names(fixture.type_hierarchy_supertypes(zed)), ["Base"]);
        assert_eq!(names(fixture.type_hierarchy_supertypes(alpha)), ["Base"]);
    }
}

#[test]
fn incompatible_compile_contexts_exclude_nodes_and_incident_edges_in_both_orders() {
    let project = TestProject::from_fixture(
        r#"
        //- /Shared.sol
        import {Base, Value} from "@dep/Types.sol";
        contract Shared is Base {
            uint256 public value;
            function inspect(Value value) external {}
        }
        contract Stable {}

        //- /left/Main.sol
        import {Shared} from "../Shared.sol";
        contract LeftChild is Shared {}

        //- /left/Types.sol
        interface Base {
            function value() external view returns (uint256);
        }
        contract Value {}

        //- /right/Main.sol
        import {Shared} from "../Shared.sol";
        contract RightChild is Shared {}

        //- /right/Types.sol
        contract Base {}
        enum Value { Item }
        "#,
    );
    let shared_path = project.path("/Shared.sol");

    for batches in [
        [("/left/Main.sol", "/left"), ("/right/Main.sol", "/right")],
        [("/right/Main.sol", "/right"), ("/left/Main.sol", "/left")],
    ] {
        let mut results = AnalysisResultAccumulator::default();
        for (entry_path, remapping_dir) in batches {
            let opts = CompileOpts {
                base_path: Some(project.root().to_path_buf()),
                import_remappings: vec![ImportRemapping {
                    context: String::new(),
                    prefix: "@dep/".into(),
                    path: project.path(remapping_dir).to_string_lossy().into_owned(),
                }],
                ..Default::default()
            };
            let entry_contents = project.read_file(entry_path);
            let entry_path = project.path(entry_path);
            results.push(analyze(AnalysisBatch::from_files(opts, [(entry_path, entry_contents)])));
        }
        let result = results.finish();
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let tables = result.symbol_tables;
        let shared_uri = Url::from_file_path(&shared_path).unwrap();

        assert_eq!(
            tables.prepare_type_hierarchy(&shared_uri, Position::new(1, 10)),
            None,
            "batch order {batches:?}"
        );
        assert_eq!(
            tables.prepare_type_hierarchy(&shared_uri, Position::new(2, 20)),
            None,
            "batch order {batches:?}"
        );
        assert_eq!(
            tables.prepare_type_hierarchy(&shared_uri, Position::new(3, 14)),
            None,
            "batch order {batches:?}"
        );
        let stable = tables
            .prepare_type_hierarchy(&shared_uri, Position::new(5, 10))
            .unwrap()
            .pop()
            .unwrap();
        assert_item(&stable, "Stable", SymbolKind::CLASS);

        for (path, child_name) in
            [("/left/Main.sol", "LeftChild"), ("/right/Main.sol", "RightChild")]
        {
            let uri = Url::from_file_path(project.path(path)).unwrap();
            let child =
                tables.prepare_type_hierarchy(&uri, Position::new(1, 10)).unwrap().pop().unwrap();
            assert_item(&child, child_name, SymbolKind::CLASS);
            assert_eq!(
                tables.type_hierarchy_supertypes(&child),
                Some(Vec::new()),
                "child {path}, batch order {batches:?}"
            );
        }

        for path in ["/left/Types.sol", "/right/Types.sol"] {
            let uri = Url::from_file_path(project.path(path)).unwrap();
            let base =
                tables.prepare_type_hierarchy(&uri, Position::new(0, 10)).unwrap().pop().unwrap();
            assert_eq!(
                tables.type_hierarchy_subtypes(&base),
                Some(Vec::new()),
                "base {path}, batch order {batches:?}"
            );
        }

        let left_uri = Url::from_file_path(project.path("/left/Types.sol")).unwrap();
        let base_getter =
            tables.prepare_type_hierarchy(&left_uri, Position::new(1, 14)).unwrap().pop().unwrap();
        assert_eq!(
            tables.type_hierarchy_subtypes(&base_getter),
            Some(Vec::new()),
            "batch order {batches:?}"
        );
    }
}

#[test]
fn conflicting_snapshots_exclude_nodes_and_incident_edges_in_both_orders() {
    let source = r#"
        //- /Shared.sol open
        contract $1Base {}

        //- /first/Main.sol
        import "../Shared.sol";
        contract $2Child is Base {}
        "#;
    let disk_contents = "contract Base { uint256 value; }\n";

    for paths in [["/first/Main.sol", "/Shared.sol"], ["/Shared.sol", "/first/Main.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Shared.sol",
            disk_contents,
            &paths,
        );

        assert_eq!(fixture.prepare_type_hierarchy("$1"), None, "batch order {paths:?}");
        let child = prepared(&fixture, "$2");
        assert_eq!(fixture.type_hierarchy_supertypes(child), Some(Vec::new()));
    }
}

#[test]
fn conflicting_request_files_cannot_leak_external_targets() {
    let source = r#"
        //- /Left.sol
        contract $1Left {}

        //- /Right.sol
        contract $2Right {}

        //- /Conflict.sol open
        import "./Left.sol";
        contract Uses is $3Left {}

        //- /DiskRoot.sol
        import "./Conflict.sol";
        "#;
    let disk_contents = "import \"./Right.sol\";\ncontract Uses is Right {}\n";

    for paths in [["/DiskRoot.sol", "/Conflict.sol"], ["/Conflict.sol", "/DiskRoot.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Conflict.sol",
            disk_contents,
            &paths,
        );
        assert_eq!(fixture.prepare_type_hierarchy("$3"), None, "batch order {paths:?}");
        assert_eq!(
            fixture.type_hierarchy_subtypes(prepared(&fixture, "$1")),
            Some(Vec::new()),
            "batch order {paths:?}"
        );
        assert_eq!(
            fixture.type_hierarchy_subtypes(prepared(&fixture, "$2")),
            Some(Vec::new()),
            "batch order {paths:?}"
        );

        let conflict_path = fixture.project_path("/Conflict.sol");
        let clean =
            analyze_tables(&conflict_path, "import \"./Left.sol\";\ncontract Uses is Left {}\n");
        let conflict_uri = Url::from_file_path(conflict_path).unwrap();
        let echoed_uses = clean
            .prepare_type_hierarchy(&conflict_uri, Position::new(1, 10))
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(fixture.type_hierarchy_supertypes(echoed_uses.clone()), None);
        assert_eq!(fixture.type_hierarchy_subtypes(echoed_uses), None);
    }
}

#[test]
fn requests_read_the_latest_published_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Hierarchy.sol
        contract Old {}
        contract SuperOld {}
        contract SuperChild is SuperOld {}
        contract SubBase {}
        contract SubOld is SubBase {}
        "#,
    );
    let path = project.path("/Hierarchy.sol");
    let old_tables = analyze_tables(&path, &project.read_file("/Hierarchy.sol"));
    let new_tables = analyze_tables(
        &path,
        "contract New {}\ncontract SuperNew {}\ncontract SuperChild is SuperNew {}\ncontract SubBase {}\ncontract SubNew is SubBase {}\n",
    );
    let uri = Url::from_file_path(path).unwrap();
    let super_child =
        old_tables.prepare_type_hierarchy(&uri, Position::new(2, 10)).unwrap().pop().unwrap();
    let sub_base =
        old_tables.prepare_type_hierarchy(&uri, Position::new(3, 10)).unwrap().pop().unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut prepare = std::pin::pin!(crate::handlers::prepare_type_hierarchy(
        &mut state,
        type_hierarchy_prepare_params(uri, Position::new(0, 10)),
    ));
    let mut supertypes = std::pin::pin!(crate::handlers::type_hierarchy_supertypes(
        &mut state,
        type_hierarchy_supertypes_params(super_child),
    ));
    let mut subtypes = std::pin::pin!(crate::handlers::type_hierarchy_subtypes(
        &mut state,
        type_hierarchy_subtypes_params(sub_base),
    ));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(prepare.as_mut().poll(&mut context).is_pending());
    assert!(supertypes.as_mut().poll(&mut context).is_pending());
    assert!(subtypes.as_mut().poll(&mut context).is_pending());

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, new_tables));
    assert!(!snapshot.publish_symbol_tables(1, SymbolTables::default()));

    assert_eq!(ready_names(prepare.as_mut().poll(&mut context)), ["New"]);
    assert_eq!(ready_names(supertypes.as_mut().poll(&mut context)), ["SuperNew"]);
    assert_eq!(ready_names(subtypes.as_mut().poll(&mut context)), ["SubNew"]);
}

#[test]
fn requests_capture_the_analysis_epoch_when_created() {
    let project = TestProject::from_fixture(
        r#"
        //- /Hierarchy.sol
        contract Base {}
        contract Child is Base {}
        "#,
    );
    let path = project.path("/Hierarchy.sol");
    let tables = analyze_tables(&path, &project.read_file("/Hierarchy.sol"));
    let uri = Url::from_file_path(path).unwrap();
    let base = tables.prepare_type_hierarchy(&uri, Position::new(0, 10)).unwrap().pop().unwrap();
    let child = tables.prepare_type_hierarchy(&uri, Position::new(1, 10)).unwrap().pop().unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = tables;

    let mut prepare = std::pin::pin!(crate::handlers::prepare_type_hierarchy(
        &mut state,
        type_hierarchy_prepare_params(uri, Position::new(0, 10)),
    ));
    let mut supertypes = std::pin::pin!(crate::handlers::type_hierarchy_supertypes(
        &mut state,
        type_hierarchy_supertypes_params(child),
    ));
    let mut subtypes = std::pin::pin!(crate::handlers::type_hierarchy_subtypes(
        &mut state,
        type_hierarchy_subtypes_params(base),
    ));

    state.mark_analysis_pending_for_test();

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert_eq!(ready_names(prepare.as_mut().poll(&mut context)), ["Base"]);
    assert_eq!(ready_names(supertypes.as_mut().poll(&mut context)), ["Base"]);
    assert_eq!(ready_names(subtypes.as_mut().poll(&mut context)), ["Child"]);
}

#[test]
fn echoed_items_follow_current_source_identity() {
    let project = TestProject::from_fixture(
        r#"
        //- /Hierarchy.sol
        contract Base {}
        contract $1Old is Base {}

        //- /Other.sol
        contract Before {}
        "#,
    );
    let hierarchy_path = project.path("/Hierarchy.sol");
    let other_path = project.path("/Other.sol");
    let old_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [
            (hierarchy_path.clone(), project.read_file("/Hierarchy.sol")),
            (other_path.clone(), project.read_file("/Other.sol")),
        ],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(&hierarchy_path).unwrap();
    let old_item =
        old_tables.prepare_type_hierarchy(&uri, Position::new(1, 10)).unwrap().pop().unwrap();

    let renamed = analyze_tables(&hierarchy_path, "contract Base {}\ncontract New is Base {}\n");
    assert_eq!(renamed.type_hierarchy_supertypes(&old_item), None);

    let moved_path = project.path("/Moved.sol");
    let moved = analyze_tables(&moved_path, "contract Base {}\ncontract Old is Base {}\n");
    assert_eq!(moved.type_hierarchy_supertypes(&old_item), None);
    assert_eq!(SymbolTables::default().type_hierarchy_supertypes(&old_item), None);

    let unrelated_change = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [
            (hierarchy_path, project.read_file("/Hierarchy.sol")),
            (other_path, "contract Changed {}\n".into()),
        ],
    ))
    .symbol_tables;
    assert_eq!(names(unrelated_change.type_hierarchy_supertypes(&old_item)), ["Base"]);
}

fn prepared(fixture: &RequestFixture, marker: &str) -> TypeHierarchyItem {
    let items = fixture.prepare_type_hierarchy(marker).unwrap();
    let [item] = items.as_slice() else {
        panic!("expected one item at marker {marker}: {items:?}")
    };
    item.clone()
}

fn names(items: Option<Vec<TypeHierarchyItem>>) -> Vec<String> {
    items.unwrap().into_iter().map(|item| item.name).collect()
}

fn assert_item(item: &TypeHierarchyItem, name: &str, kind: SymbolKind) {
    assert_eq!(item.name, name);
    assert_eq!(item.kind, kind);
    assert_eq!(item.tags, None);
    assert_eq!(item.detail, None);
    assert!(item.range.start <= item.selection_range.start);
    assert!(item.selection_range.end <= item.range.end);
}

fn analyze_tables(path: &std::path::Path, source: &str) -> SymbolTables {
    analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.to_path_buf(), source.to_owned())],
    ))
    .symbol_tables
}

fn ready_names(
    poll: Poll<Result<Option<Vec<TypeHierarchyItem>>, async_lsp::ResponseError>>,
) -> Vec<String> {
    let Poll::Ready(response) = poll else { panic!("request should be ready") };
    names(response.unwrap())
}
