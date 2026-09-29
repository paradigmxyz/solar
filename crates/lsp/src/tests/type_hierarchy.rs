use super::*;
use lsp_types::{SymbolKind, SymbolTag, TypeHierarchyItem};
use solar_config::ImportRemapping;
use std::pin::Pin;

#[test]
fn contract_edges_are_direct_in_diamonds_and_multilevel_hierarchies() {
    let fixture = RequestFixture::new(
        r#"
        //- /Diamond.sol
        contract $1Root {}
        contract $2Left is $5Root {}
        contract $3Right is Root {}
        contract $4Leaf is Left, Right {}
        "#,
        "/Diamond.sol",
    );
    let root = prepared(&fixture, "$1");
    assert_eq!(prepared(&fixture, "$5"), root);
    assert_eq!(root.name, "Root");

    assert_eq!(fixture.type_hierarchy_supertypes(prepared(&fixture, "$2")), Some(vec![root]));
    assert!(supertypes(&fixture, "$1").is_empty());
    assert_eq!(supertypes(&fixture, "$3"), ["Root"]);
    assert_eq!(supertypes(&fixture, "$4"), ["Left", "Right"]);
    assert_eq!(subtypes(&fixture, "$1"), ["Left", "Right"]);
    assert_eq!(subtypes(&fixture, "$2"), ["Leaf"]);
    assert_eq!(subtypes(&fixture, "$3"), ["Leaf"]);
    assert!(subtypes(&fixture, "$4").is_empty());
}

#[test]
fn presents_all_supported_declarations_and_callable_edges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Callables.sol
        function $1freeFunction(uint256 value) pure returns (uint256) { return value; }
        function callFreeFunction() pure returns (uint256) { return $16freeFunction(1); }
        interface $2Iface {
            function $19total() external view returns (uint256);
        }
        library $3Lib {}

        abstract contract $4Base {
            function $5value() external view virtual returns (uint256);
            function $6run(uint256 value) public virtual returns (uint256) { return value; }
            modifier $7guard() virtual { _; }
            $21fallback() external virtual {}
            $22receive() external payable virtual {}
        }

        contract $8Derived is Base, Iface {
            uint256 public override $9value;
            uint256 public $20total;

            $10constructor(uint256 initial) { value = initial; }
            $11fallback() external override {}
            $12receive() external payable override {}

            function $13run(uint256 value_) public pure override returns (uint256) {
                return value_;
            }

            modifier $14guard() override { _; }

            function protected() public $17guard {}
            function $18hidden() private {}

            function read() external view returns (uint256) {
                return this.$15value();
            }

            // Unsupported declarations.
            uint256 public $24plain;
            uint256 private $25secret;
            struct $26Data { uint256 value; }
            enum $27Choice { A }
            event $28Changed(uint256 value);
            error $29Failed();

            function use(uint256 $30parameter) public pure {
                uint256 $31local = parameter;
                assembly {
                    function $32yulFunction() -> result { result := 1 }
                    pop(yulFunction())
                }
            }
        }
        type $23Value is uint256;
        "#,
        "/Callables.sol",
    );

    for (marker, name, kind) in [
        ("$1", "freeFunction(uint256)", SymbolKind::FUNCTION),
        ("$2", "Iface", SymbolKind::INTERFACE),
        ("$3", "Lib", SymbolKind::MODULE),
        ("$4", "Base", SymbolKind::CLASS),
        ("$5", "Base.value()", SymbolKind::METHOD),
        ("$6", "Base.run(uint256)", SymbolKind::METHOD),
        ("$7", "Base.guard", SymbolKind::FUNCTION),
        ("$8", "Derived", SymbolKind::CLASS),
        ("$9", "Derived.value", SymbolKind::PROPERTY),
        ("$10", "Derived.constructor(uint256)", SymbolKind::CONSTRUCTOR),
        ("$11", "Derived.fallback()", SymbolKind::FUNCTION),
        ("$12", "Derived.receive()", SymbolKind::FUNCTION),
        ("$13", "Derived.run(uint256)", SymbolKind::METHOD),
        ("$14", "Derived.guard", SymbolKind::FUNCTION),
        ("$18", "Derived.hidden()", SymbolKind::METHOD),
    ] {
        assert_item(&prepared(&fixture, marker), name, kind);
    }
    // Items use the exact declaration range and the name or keyword as the selection range.
    let range = |start: (u32, u32), end: (u32, u32)| {
        Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1))
    };
    for (marker, full, selection) in [
        ("$2", range((2, 0), (4, 1)), range((2, 10), (2, 15))),
        ("$10", range((16, 4), (16, 53)), range((16, 4), (16, 15))),
        ("$11", range((17, 4), (17, 35)), range((17, 4), (17, 12))),
        ("$12", range((18, 4), (18, 42)), range((18, 4), (18, 11))),
    ] {
        let item = prepared(&fixture, marker);
        assert_eq!((item.range, item.selection_range), (full, selection), "marker {marker}");
    }
    for marker in 23..=32 {
        assert_eq!(fixture.prepare_type_hierarchy(&format!("${marker}")), None, "marker {marker}");
    }
    assert_eq!(prepared(&fixture, "$15"), prepared(&fixture, "$9"));
    assert_eq!(prepared(&fixture, "$16"), prepared(&fixture, "$1"));
    assert_eq!(prepared(&fixture, "$17"), prepared(&fixture, "$14"));

    for (derived, base, derived_name, base_name) in [
        ("$9", "$5", "Derived.value", "Base.value()"),
        ("$20", "$19", "Derived.total", "Iface.total()"),
        ("$13", "$6", "Derived.run(uint256)", "Base.run(uint256)"),
        ("$14", "$7", "Derived.guard", "Base.guard"),
        ("$11", "$21", "Derived.fallback()", "Base.fallback()"),
        ("$12", "$22", "Derived.receive()", "Base.receive()"),
    ] {
        assert_eq!(supertypes(&fixture, derived), [base_name]);
        assert_eq!(subtypes(&fixture, base), [derived_name]);
    }
}

#[test]
fn validates_the_full_echoed_item_and_opaque_data() {
    let fixture = RequestFixture::new(
        r#"
        //- /Validation.sol
        contract $1Base {}
        contract $2Child is Base {}
        "#,
        "/Validation.sol",
    );
    let item = prepared(&fixture, "$1");
    let range = item.selection_range;
    let data = |version, uri: &Url| {
        json!([
            version,
            uri,
            range.start.line,
            range.start.character,
            range.end.line,
            range.end.character,
        ])
    };
    assert_eq!(item.data, Some(data(2, &item.uri)));

    let other_uri = Url::from_file_path(std::env::temp_dir().join("Other.sol")).unwrap();
    let changed = |item: &TypeHierarchyItem, change: &dyn Fn(&mut TypeHierarchyItem)| {
        let mut item = item.clone();
        change(&mut item);
        item
    };
    let mut tampered = vec![
        changed(&prepared(&fixture, "$2"), &|changed| changed.data = item.data.clone()),
        changed(&item, &|changed| changed.name.push_str("Changed")),
        changed(&item, &|changed| changed.kind = SymbolKind::INTERFACE),
        changed(&item, &|changed| changed.tags = Some(SymbolTag::DEPRECATED)),
        changed(&item, &|changed| changed.detail = Some("changed".into())),
        changed(&item, &|changed| changed.uri = other_uri.clone()),
        changed(&item, &|changed| changed.range.end.character += 1),
        changed(&item, &|changed| changed.selection_range.end.character += 1),
    ];

    // URL parsing normalizes the scheme, but echoed data must keep the exact serialized spelling.
    let normalized_uri = item.uri.as_str().replacen("file:", "FILE:", 1);
    assert_eq!(Url::parse(&normalized_uri).unwrap(), item.uri);
    tampered.push(changed(&item, &|changed| {
        changed.data.as_mut().unwrap()[1] = json!(normalized_uri);
    }));
    for index in [0, 2, 3, 4, 5] {
        // JSON floats and strings must not be accepted as integer version or position fields.
        let field = &item.data.as_ref().unwrap()[index];
        for value in [json!(field.as_u64().unwrap() as f64), json!(field.to_string())] {
            tampered.push(changed(&item, &|changed| {
                changed.data.as_mut().unwrap()[index] = value.clone();
            }));
        }
    }

    for data in [
        None,
        Some(json!(null)),
        Some(json!([])),
        Some(data(1, &item.uri)),
        Some(data(2, &other_uri)),
        Some(json!([2, item.uri, 9, 0, 9, 1])),
        Some(json!([2, item.uri, 0, 0, 0, 0, true])),
        Some(json!([2, item.uri, 0, 0, 0])),
        Some(json!([2, item.uri, -1, 0, 0, 0])),
        Some(json!([2, item.uri, 4294967296u64, 0, 0, 0])),
        Some(json!([2, item.uri, 0.0, 0, 0, 0])),
        Some(json!({
            "version": 1,
            "uri": item.uri,
            "selectionRange": item.selection_range,
        })),
    ] {
        tampered.push(changed(&item, &|changed| changed.data = data.clone()));
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

        type Amount is uint256;

        contract C {
            struct Data { uint256 value; }
            enum Choice { A }

            function $9choose(Base value) internal {}
            function $10choose(Leaf value) internal {}
            function $11inspect(Data memory value) internal {}
            function $12inspect(Choice value) internal {}
            function $13inspect(Amount value) internal {}
        }
        "#,
        "/Overrides.sol",
    );

    assert_eq!(prepared(&fixture, "$7"), prepared(&fixture, "$5"));
    assert_eq!(prepared(&fixture, "$8"), prepared(&fixture, "$6"));
    assert_eq!(subtypes(&fixture, "$1"), ["Middle.run(uint256)"]);
    assert_eq!(subtypes(&fixture, "$2"), ["Middle.run(address)"]);
    assert_eq!(supertypes(&fixture, "$5"), ["Middle.run(uint256)"]);
    assert_eq!(supertypes(&fixture, "$6"), ["Middle.run(address)"]);
    assert_eq!(subtypes(&fixture, "$3"), ["Leaf.run(uint256)"]);
    assert_eq!(subtypes(&fixture, "$4"), ["Leaf.run(address)"]);
    // Canonical names keep user-defined parameter types distinct.
    assert_eq!(
        ["$9", "$10", "$11", "$12", "$13"].map(|marker| prepared(&fixture, marker).name),
        [
            "C.choose(contract Base)",
            "C.choose(contract Leaf)",
            "C.inspect(struct C.Data)",
            "C.inspect(enum C.Choice)",
            "C.inspect(Amount)",
        ]
    );
}

#[test]
fn merges_identical_cross_batch_nodes_and_edges_in_both_orders() {
    let source = r#"
        //- /Shared.sol
        contract $1Base {}
        contract Holder { $4Base value; }

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

        assert_eq!(prepared(&fixture, "$4"), prepared(&fixture, "$1"));
        assert_eq!(subtypes(&fixture, "$1"), ["Zed", "Alpha"], "batch order {paths:?}");
        assert_eq!(supertypes(&fixture, "$2"), ["Base"]);
        assert_eq!(supertypes(&fixture, "$3"), ["Base"]);
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
    let uri = |path| project.uri(path);
    let shared_uri = uri("/Shared.sol");

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
                    path: format!("{}/", project.path(remapping_dir).display()),
                }],
                ..Default::default()
            };
            let entry = (project.path(entry_path), project.read_file(entry_path));
            results.push(analyze(AnalysisBatch::from_files(opts, [entry])));
        }
        let result = results.finish();
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let tables = result.symbol_tables;
        let item = |uri: &Url, line, character| item_at(&tables, uri, line, character);

        // `Shared`, its `Base` reference, its getter, `inspect`, and its `Value` reference.
        for (line, character) in [(1, 10), (1, 20), (2, 20), (3, 14), (3, 22)] {
            assert_eq!(
                tables.prepare_type_hierarchy(&shared_uri, Position::new(line, character)),
                None,
                "{line}:{character}, batch order {batches:?}"
            );
        }
        assert_item(&item(&shared_uri, 5, 10), "Stable", SymbolKind::CLASS);

        for (path, child_name) in
            [("/left/Main.sol", "LeftChild"), ("/right/Main.sol", "RightChild")]
        {
            let child = item(&uri(path), 1, 10);
            assert_item(&child, child_name, SymbolKind::CLASS);
            assert_eq!(tables.type_hierarchy_supertypes(&child), Some(Vec::new()), "{path}");
        }
        // Both `Base` declarations, then the interface getter of `Base.value`.
        for (path, line, character) in
            [("/left/Types.sol", 0, 10), ("/right/Types.sol", 0, 10), ("/left/Types.sol", 1, 14)]
        {
            let base = item(&uri(path), line, character);
            assert_eq!(tables.type_hierarchy_subtypes(&base), Some(Vec::new()), "{path}");
        }
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
        assert!(supertypes(&fixture, "$2").is_empty());
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
        assert!(subtypes(&fixture, "$1").is_empty(), "batch order {paths:?}");
        assert!(subtypes(&fixture, "$2").is_empty(), "batch order {paths:?}");

        let conflict_path = fixture.project_path("/Conflict.sol");
        let clean =
            analyze_tables(&conflict_path, "import \"./Left.sol\";\ncontract Uses is Left {}\n");
        let conflict_uri = fixture.project().uri("/Conflict.sol");
        let echoed_uses = item_at(&clean, &conflict_uri, 1, 10);
        assert_eq!(fixture.type_hierarchy_supertypes(echoed_uses.clone()), None);
        assert_eq!(fixture.type_hierarchy_subtypes(echoed_uses), None);
    }
}

#[test]
fn requests_reject_superseded_analysis() {
    let project = TestProject::new();
    let path = project.path("/Hierarchy.sol");
    let old_tables = analyze_tables(
        &path,
        "contract Old {}\ncontract SuperOld {}\ncontract SuperChild is SuperOld {}\ncontract SubBase {}\ncontract SubOld is SubBase {}\n",
    );
    let new_tables = analyze_tables(
        &path,
        "contract New {}\ncontract SuperNew {}\ncontract SuperChild is SuperNew {}\ncontract SubBase {}\ncontract SubNew is SubBase {}\n",
    );
    let uri = project.uri("/Hierarchy.sol");

    // Analysis superseded before the requests are polled.
    let mut state = state_with(Config::default());
    let requests = hierarchy_requests(&mut state, old_tables.clone(), &uri, (2, 10), (3, 10));
    state.mark_analysis_pending_for_test();
    assert_content_modified(requests);

    let mut state = state_with(Config::default());
    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut requests = hierarchy_requests(&mut state, old_tables, &uri, (2, 10), (3, 10));
    for request in &mut requests {
        assert_polls(true, request.as_mut());
    }

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, Arc::new(new_tables)));
    assert!(!snapshot.publish_symbol_tables(1, Default::default()));

    // A new publication must not retarget an old request.
    assert_content_modified(requests);
}

#[test]
fn echoed_items_follow_current_source_identity() {
    let project = TestProject::from_fixture(
        r#"
        //- /Hierarchy.sol
        contract Base {}
        contract Old is Base {}

        //- /Other.sol
        contract Before {}
        "#,
    );
    let hierarchy_path = project.path("/Hierarchy.sol");
    let other_path = project.path("/Other.sol");
    let analyze_both = |other: String| {
        analyze(AnalysisBatch::from_files(
            CompileOpts::default(),
            [
                (hierarchy_path.clone(), project.read_file("/Hierarchy.sol")),
                (other_path.clone(), other),
            ],
        ))
        .symbol_tables
    };
    let old_tables = analyze_both(project.read_file("/Other.sol"));
    let uri = project.uri("/Hierarchy.sol");
    let old_item = item_at(&old_tables, &uri, 1, 10);

    let renamed = analyze_tables(&hierarchy_path, "contract Base {}\ncontract New is Base {}\n");
    assert_eq!(renamed.type_hierarchy_supertypes(&old_item), None);

    let moved_path = project.path("/Moved.sol");
    let moved = analyze_tables(&moved_path, "contract Base {}\ncontract Old is Base {}\n");
    assert_eq!(moved.type_hierarchy_supertypes(&old_item), None);
    assert_eq!(SymbolTables::default().type_hierarchy_supertypes(&old_item), None);

    let unrelated_change = analyze_both("contract Changed {}\n".into());
    assert_eq!(names(unrelated_change.type_hierarchy_supertypes(&old_item)), ["Base"]);
}

type HierarchyRequest =
    Pin<Box<dyn Future<Output = Result<Option<Vec<TypeHierarchyItem>>, ResponseError>>>>;

/// Starts prepare, supertypes, and subtypes requests against `tables`.
fn hierarchy_requests(
    state: &mut GlobalState,
    tables: SymbolTables,
    uri: &Url,
    (super_line, super_character): (u32, u32),
    (sub_line, sub_character): (u32, u32),
) -> [HierarchyRequest; 3] {
    let supertypes =
        from_json(json!({ "item": item_at(&tables, uri, super_line, super_character) }));
    let subtypes = from_json(json!({ "item": item_at(&tables, uri, sub_line, sub_character) }));
    state.symbol_tables.store(Arc::new(tables));
    let prepare = request_params(uri, Position::new(0, 10), json!({}));
    [
        Box::pin(crate::handlers::prepare_type_hierarchy(state, prepare)),
        Box::pin(crate::handlers::type_hierarchy_supertypes(state, supertypes)),
        Box::pin(crate::handlers::type_hierarchy_subtypes(state, subtypes)),
    ]
}

fn assert_content_modified(requests: [HierarchyRequest; 3]) {
    for request in requests {
        let error = expect_ready(request).expect_err("superseded requests should return an error");
        assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    }
}

fn prepared(fixture: &RequestFixture, marker: &str) -> TypeHierarchyItem {
    let items = fixture.prepare_type_hierarchy(marker).unwrap();
    let [item] = items.as_slice() else {
        panic!("expected one item at marker {marker}: {items:?}")
    };
    item.clone()
}

fn supertypes(fixture: &RequestFixture, marker: &str) -> Vec<String> {
    names(fixture.type_hierarchy_supertypes(prepared(fixture, marker)))
}

fn subtypes(fixture: &RequestFixture, marker: &str) -> Vec<String> {
    names(fixture.type_hierarchy_subtypes(prepared(fixture, marker)))
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

fn item_at(tables: &SymbolTables, uri: &Url, line: u32, character: u32) -> TypeHierarchyItem {
    tables.prepare_type_hierarchy(uri, Position::new(line, character)).unwrap().pop().unwrap()
}

fn analyze_tables(path: &Path, source: &str) -> SymbolTables {
    analyze_source(path, source).symbol_tables
}
