use super::{call_hierarchy::merge_symbol_tables, *};
use crate::config::CodeLensConfig;
use snapbox::{assert_data_eq, str};

#[test]
fn keeps_annotations_around_broken_statements() {
    for broken in [
        "uint x = 1 + * 2;",
        "if (true) { uint x = 1 + * 2; }",
        "for (uint i = * 2; i < 3; i++) {}",
        "assembly { let x := add(1, *) }",
        "uint x = missing;",
        "uint x = 1",
    ] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Editing.sol
                contract C {{
                    function target(uint amount) public pure returns (uint) {{ return amount; }}
                    function caller() public pure {{
                        target(1);
                        {broken}
                        target(2);
                    }}
                    function afterError() external {{}}
                }}
                "#
            ),
            "/Editing.sol",
        );
        fixture.check_inlay_hints(
            "/Editing.sol",
            str![[r#"
3:15 PARAMETER amount:
3:17 TYPE : uint256
5:15 PARAMETER amount:
5:17 TYPE : uint256

"#]],
        );
        fixture.check_code_lenses(
            "/Editing.sol",
            str![[r#"
0:9 references=0 command=<none>
1:13 references=2 command=solar.showReferences
1:13 selector=0x9811c7c1 command=solar.copySelector
1:25 references=1 command=solar.showReferences
2:13 references=0 command=<none>
2:13 selector=0xfc9c8d39 command=solar.copySelector
7:13 references=0 command=<none>
7:13 selector=0x317c90e4 command=solar.copySelector

"#]],
        );
    }
}

#[test]
fn keeps_selectors_around_broken_declarations() {
    for broken in [
        "uint broken = * 2;",
        "function broken(uint value +) external {}",
        "???;",
        "function broken() external { uint x = * 2;",
    ] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Editing.sol
                contract C {{
                    function beforeError() external {{}}
                    {broken}
                    function afterError() external {{}}
                }}
                "#
            ),
            "/Editing.sol",
        );
        let state = fixture.state();
        let uri = Url::from_file_path(fixture.project_path("/Editing.sol")).unwrap();
        let lenses = state.symbol_tables.load().code_lenses(&uri, CodeLensConfig::default());
        let selectors = lenses
            .iter()
            .filter(|lens| matches!(lens.range.start.line, 1 | 3))
            .filter_map(|lens| lens.command.as_ref())
            .map(|command| command.title.as_str())
            .collect::<Vec<_>>();
        assert_data_eq!(
            selectors.join("\n"),
            str![[r#"
0 references
0xe357ca3c
0 references
0x317c90e4"#]],
        );
    }
}

#[test]
fn keeps_existing_selectors_while_typing_a_new_function() {
    let declaration = "function editing() external { if (true) { uint x = 1 + * 2; } assembly { let y := add(1, 2) } }";
    for end in 0..=declaration.len() {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                "//- /Editing.sol\ncontract C {{\nfunction beforeError() external {{}}\n{}\n}}\n",
                &declaration[..end],
            ),
            "/Editing.sol",
        );
        let state = fixture.state();
        let uri = Url::from_file_path(fixture.project_path("/Editing.sol")).unwrap();
        let options = CodeLensConfig { references: false, ..Default::default() };
        let lenses = state.symbol_tables.load().code_lenses(&uri, options);
        let titles = lenses
            .iter()
            .filter(|lens| lens.range.start.line == 1)
            .filter_map(|lens| lens.command.as_ref())
            .map(|command| command.title.as_str())
            .collect::<Vec<_>>();
        assert_data_eq!(titles.join("\n"), str![["0xe357ca3c"]]);
    }
}

#[test]
fn shows_selectors_and_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /CodeLens.sol
        contract Token {
            uint256 public value;

            function transfer(address to, uint256 amount)
                public
                returns (uint256 result)
            {
                value = amount;
                return amount;
            }

            function callTransfer(address target) external {
                uint256 local = value;
                assembly {
                    function inner(y) -> z { z := y }
                    local := inner(local)
                }
                transfer(target, local);
            }

            // Only public and external functions and getters have selectors.
            uint256 private hidden;
            function internalFn() internal {}
            function privateFn() private {}
            constructor() {}
            fallback() external {}
            receive() external payable {}
        }
        "#,
        "/CodeLens.sol",
    );

    fixture.check_code_lenses(
        "/CodeLens.sol",
        str![[r#"
0:9 references=0 command=<none>
1:19 references=2 command=solar.showReferences
1:19 selector=0x3fa4f245 command=solar.copySelector
2:13 references=1 command=solar.showReferences
2:13 selector=0xa9059cbb command=solar.copySelector
2:30 references=0 command=<none>
2:42 references=2 command=solar.showReferences
9:13 references=0 command=<none>
9:13 selector=0xeec990f2 command=solar.copySelector
9:34 references=1 command=solar.showReferences
18:20 references=0 command=<none>
19:13 references=0 command=<none>
20:13 references=0 command=<none>
21:4 references=0 command=<none>
22:4 references=0 command=<none>
23:4 references=0 command=<none>

"#]],
    );
}

#[test]
fn shows_direct_inheritance_counts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hierarchy.sol
        contract Base {}
        contract Mid is Base {}
        contract Leaf is Mid {
            function target() public {}
            function callTarget() external { target(); }
        }
        "#,
        "/Hierarchy.sol",
    );

    fixture.check_code_lenses(
        "/Hierarchy.sol",
        str![[r#"
0:9 references=1 command=solar.showReferences
0:9 inheritance=1 derived contract command=solar.showTypeHierarchy
1:9 references=1 command=solar.showReferences
1:9 inheritance=1 base contract command=solar.showTypeHierarchy
1:9 inheritance=1 derived contract command=solar.showTypeHierarchy
2:9 references=0 command=<none>
2:9 inheritance=1 base contract command=solar.showTypeHierarchy
3:13 references=1 command=solar.showReferences
3:13 selector=0xd4b83992 command=solar.copySelector
4:13 references=0 command=<none>
4:13 selector=0x2872b1ff command=solar.copySelector

"#]],
    );
    fixture.check_code_lenses_without_commands(
        "/Hierarchy.sol",
        str![[r#"
0:9 references=1 command=<none>
0:9 inheritance=1 derived contract command=<none>
1:9 references=1 command=<none>
1:9 inheritance=1 base contract command=<none>
1:9 inheritance=1 derived contract command=<none>
2:9 references=0 command=<none>
2:9 inheritance=1 base contract command=<none>
3:13 references=1 command=<none>
3:13 selector=0xd4b83992 command=<none>
4:13 references=0 command=<none>
4:13 selector=0x2872b1ff command=<none>

"#]],
    );
}

#[test]
fn snapshots_complete_command_protocol() {
    let fixture = RequestFixture::new(
        r#"
        //- /Protocol.sol
        contract Base {}
        contract Plain is Base {
            function target() public {}
        }
        "#,
        "/Protocol.sol",
    );

    fixture.check_code_lenses_json("/Protocol.sol", str![[r#"
{"range":{"start":{"line":0,"character":9},"end":{"line":0,"character":13}},"command":{"title":"1 reference","command":"solar.showReferences","arguments":[{"position":{"character":9,"line":0},"uri":"file:///Protocol.sol"}]}}
{"range":{"start":{"line":0,"character":9},"end":{"line":0,"character":13}},"command":{"title":"1 derived contract","command":"solar.showTypeHierarchy","arguments":[{"direction":"subtypes","position":{"character":9,"line":0},"uri":"file:///Protocol.sol"}]}}
{"range":{"start":{"line":1,"character":9},"end":{"line":1,"character":14}},"command":{"title":"0 references","command":""}}
{"range":{"start":{"line":1,"character":9},"end":{"line":1,"character":14}},"command":{"title":"1 base contract","command":"solar.showTypeHierarchy","arguments":[{"direction":"supertypes","position":{"character":9,"line":1},"uri":"file:///Protocol.sol"}]}}
{"range":{"start":{"line":2,"character":13},"end":{"line":2,"character":19}},"command":{"title":"0 references","command":""}}
{"range":{"start":{"line":2,"character":13},"end":{"line":2,"character":19}},"command":{"title":"0xd4b83992","command":"solar.copySelector","arguments":["0xd4b83992"]}}

"#]]);
}

#[test]
fn merges_reference_counts_for_imported_declarations() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Shared.sol
        contract $1Base {
            function $2ping() public {}
        }

        //- /first/Main.sol
        import "../Shared.sol";
        contract First {
            function useBase() external {
                Base value;
                value.ping();
            }
        }

        //- /second/Main.sol
        import "../Shared.sol";
        contract Second {
            function useBase() external {
                Base value;
                value.ping();
            }
        }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_code_lenses(
        "/Shared.sol",
        str![[r#"
0:9 references=2 command=solar.showReferences
1:13 references=2 command=solar.showReferences
1:13 selector=0x5c36b186 command=solar.copySelector

"#]],
    );
    fixture.check_queries(
        &[Query::References(false)],
        [1, 2],
        str![[r#"
$1 /first/Main.sol:3:8 Base value;
/second/Main.sol:3:8 Base value;
$2 /first/Main.sol:4:14 value.ping();
/second/Main.sol:4:14 value.ping();

"#]],
    );
}

#[test]
fn recomputes_warmed_reference_counts_when_merging_batches() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Shared.sol
        library Shared {
            function $1ping() internal pure {}
        }

        //- /First.sol
        import "./Shared.sol";
        library First {
            function callFirst() internal pure { Shared.ping(); }
        }

        //- /Second.sol
        import "./First.sol";
        contract Second {
            function callSecond() external pure {
                First.callFirst();
                Shared.ping();
            }
        }
        "#,
    );
    let project = marked.project();
    let first = analyze_file(&marked, "/First.sol");
    let second = analyze_file(&marked, "/Second.sol");
    let uri = project.uri("/Shared.sol");
    let position = marked.marker("$1").position();

    assert_data_eq!(lens_titles_at(&first, &uri, position), "1 reference\n");
    assert_data_eq!(lens_titles_at(&second, &uri, position), "2 references\n");
    // The shared caller appears in both batches, but contributes only one location.
    check_merged_titles(&first, &second, &uri, position, "2 references\n");
}

#[test]
fn suppresses_warmed_reference_counts_after_merging_conflicting_callers() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Target.sol
        library Target {
            function $1target() external pure {}
        }

        //- /Caller.sol
        import "./Target.sol";
        contract Caller {
            function callTarget() external pure { Target.target(); }
        }

        //- /Root.sol
        import "./Caller.sol";
        "#,
    );
    let project = marked.project();
    let contents = project.read_file("/Caller.sol");
    let first = analyze_file(&marked, "/Caller.sol");
    project.write_file("/Caller.sol", &contents.replace("Target.target();", "\nTarget.target();"));
    let second = analyze_file(&marked, "/Root.sol");
    let uri = project.uri("/Target.sol");
    let position = marked.marker("$1").position();

    assert_data_eq!(lens_titles_at(&first, &uri, position), "1 reference\n0xd4b83992\n");
    assert_data_eq!(lens_titles_at(&second, &uri, position), "1 reference\n0xd4b83992\n");
    check_merged_titles(&first, &second, &uri, position, "0xd4b83992\n");
}

#[test]
fn rejects_references_from_conflicting_source_snapshots() {
    let source = r#"
        //- /Target.sol
        library Target {
            function $1target() external pure {}
        }

        //- /Caller.sol open
        import {Target} from "./Target.sol";

        contract C {
            function caller() external pure {
                Target.target();
            }
        }

        //- /Root.sol
        import "./Caller.sol";
        "#;
    let disk_contents = r#"import {Target} from "./Target.sol";

contract C {
    function caller() external pure {

        Target.target();
    }
}
"#;

    for paths in [["/Root.sol", "/Caller.sol"], ["/Caller.sol", "/Root.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Caller.sol",
            disk_contents,
            &paths,
        );

        fixture.check_code_lenses(
            "/Target.sol",
            str![[r#"
1:13 selector=0xd4b83992 command=solar.copySelector

"#]],
        );
        fixture.check_references("$1", false, "<none>\n");
    }
}

#[test]
fn skips_selectors_for_invalid_signatures() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Invalid.sol
        contract Invalid {
            function bad(Unknown value) external {}
        }
        "#,
        "/Invalid.sol",
    );

    fixture.check_code_lenses(
        "/Invalid.sol",
        str![[r#"
0:9 references=0 command=<none>
1:13 references=0 command=<none>
1:25 references=0 command=<none>

"#]],
    );
}

fn lens_titles_at(tables: &SymbolTables, uri: &Url, position: Position) -> String {
    let mut output = String::new();
    for lens in tables.code_lenses(uri, CodeLensConfig::default()) {
        if lens.range.start == position {
            output.push_str(&lens.command.unwrap().title);
            output.push('\n');
        }
    }
    output
}

/// Checks lens titles after merging in both batch orders, including repeated warmed requests.
fn check_merged_titles(
    first: &SymbolTables,
    second: &SymbolTables,
    uri: &Url,
    position: Position,
    expected: &str,
) {
    for (first, second) in [(first, second), (second, first)] {
        let tables = merge_symbol_tables(first.clone(), second.clone());
        for _ in 0..2 {
            assert_data_eq!(lens_titles_at(&tables, uri, position), expected);
        }
    }
}

fn analyze_file(marked: &MarkedProject, path: &str) -> SymbolTables {
    let project = marked.project();
    let result = analyze_source(project.path(path), project.read_file(path));
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    result.symbol_tables
}
