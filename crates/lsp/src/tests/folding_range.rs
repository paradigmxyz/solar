use super::support::{RequestFixture, folding_range_output};
use crate::folding_range::{folding_ranges, folding_ranges_from_rope};
use crop::Rope;
use lsp_types::Url;
use snapbox::{IntoData, assert_data_eq, str};

#[test]
fn serves_open_disk_and_empty_documents_without_waiting_for_analysis() {
    let fixture = RequestFixture::new(
        r#"
        //- /Open.sol open
        contract Open {
            uint256 value;
        }

        //- /Disk.sol
        contract Disk {
            uint256 value;
        }

        //- /Empty.sol open
        "#,
        "/Empty.sol",
    );
    fixture.write_file("/Open.sol", "contract Disk {}");

    fixture.check_folding_ranges("/Open.sol", "0:0-2:1 code\n");
    fixture.check_folding_ranges("/Disk.sol", "0:0-2:1 code\n");
    fixture.check_folding_range_uses_blocking_pool("/Open.sol", "0:0-2:1 code\n");
    fixture.check_folding_ranges("/Empty.sol", str![""]);
    assert_eq!(fixture.folding_ranges(Url::parse("untitled:Folding.sol").unwrap()), None);
    let missing = Url::from_file_path(fixture.project_path("/Missing.sol")).unwrap();
    assert_eq!(fixture.folding_ranges(missing), None);
}

#[test]
fn folds_parsed_sources() {
    for (source, expected) in [
        // Folds declarations and nested Solidity blocks.
        (
            concat!(
                "contract C {\n",
                "    function f() external {\n",
                "        if (true) {\n",
                "            {\n",
                "                uint256 x;\n",
                "            }\n",
                "        }\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
0:0-8:1 code
1:4-7:5 code
2:18-6:9 code
3:12-5:13 code

"#]],
        ),
        // Folds full multiline named declaration ranges.
        (
            concat!(
                "interface I {\n",
                "    event Changed(\n",
                "        uint256 value\n",
                "    );\n",
                "\n",
                "    function read(\n",
                "        uint256 key\n",
                "    ) external view returns (\n",
                "        uint256 value\n",
                "    );\n",
                "}\n",
            ),
            str![[r#"
0:0-10:1 code
1:4-3:6 code
5:4-9:6 code

"#]],
        ),
        // Folds comments at every nesting level and splits groups on blank lines.
        (
            concat!(
                "// alpha\n",
                "// beta\n",
                "\n",
                "/// gamma\n",
                "// delta\n",
                "contract C {\n",
                "    /* nested\n",
                "       block */\n",
                "    function f() external {\n",
                "        // inner\n",
                "        // group\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
0:0-1:7 comment
3:0-4:8 comment
5:0-12:1 code
6:4-7:15 comment
8:4-11:5 code
9:8-10:16 comment

"#]],
        ),
        // Folds import groups and splits them on blank lines or items.
        (
            concat!(
                "import \"a.sol\";\n",
                "import {A} from \"b.sol\";\n",
                "// keep this group together\n",
                "import \"c.sol\";\n",
                "\n",
                "import \"d.sol\";\n",
                "import \"e.sol\";\n",
                "pragma solidity ^0.8.0;\n",
                "import \"f.sol\";\n",
                "import \"g.sol\";\n",
            ),
            str![[r#"
0:0-3:15 imports
5:0-6:15 imports
8:0-9:15 imports

"#]],
        ),
        // Folds Yul declarations and nested bodies.
        (
            concat!(
                "contract C {\n",
                "    function f() external {\n",
                "        assembly {\n",
                "            function y(x) -> r {\n",
                "                if x {\n",
                "                    r := x\n",
                "                }\n",
                "            }\n",
                "            {\n",
                "                let z := 1\n",
                "            }\n",
                "            switch x\n",
                "            case 0 {\n",
                "                pop(0)\n",
                "            }\n",
                "            default {\n",
                "                pop(1)\n",
                "            }\n",
                "        }\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
0:0-20:1 code
1:4-19:5 code
2:17-18:9 code
3:12-7:13 code
4:21-6:17 code
8:12-10:13 code
12:19-14:13 code
15:20-17:13 code

"#]],
        ),
        // Uses UTF-16 positions and CRLF line endings.
        (
            concat!("😀 /* first\r\n", "second */\r\n", "// 一😀\r\n", "// 二😀\r\n",),
            str![[r#"
0:3-1:9 comment
2:0-3:6 comment

"#]],
        ),
        // Single-line sources have no folding ranges.
        ("contract C { function f() external {} }", str![""]),
    ] {
        check(source, expected);
    }
}

#[test]
fn recovers_folding_ranges_after_parse_errors() {
    for (source, expected) in [
        // Falls back to import groups after parse errors.
        (
            concat!("@ invalid\n", "import \"a.sol\";\n", "import \"b.sol\";\n",),
            str![[r#"
1:0-2:15 imports

"#]],
        ),
        // Lexical import fallback ignores member accesses.
        (
            concat!("uint256 constant X = Foo.import\n", "    + 1;\n", "@ invalid\n",),
            str![[r#"
0:0-1:8 code

"#]],
        ),
        // Extends recognized incomplete blocks to the physical EOF.
        (
            concat!(
                "contract C {\n",
                "    function f() external {\n",
                "        if (true) {\n",
                "            uint256 x\n",
                "            // trailing comment\n",
            ),
            str![[r#"
0:0-5:0 code
1:4-5:0 code
2:18-5:0 code

"#]],
        ),
        // Falls back to recognized blocks when parsing fails.
        (
            concat!(
                "@ invalid\n",
                "contract Broken {\n",
                "    function f() external {\n",
                "        if (true) {\n",
            ),
            str![[r#"
1:0-4:0 code
2:4-4:0 code
3:18-4:0 code

"#]],
        ),
        // Lexical fallback recognizes incomplete Yul `for` post blocks.
        (
            concat!(
                "@ invalid\n",
                "contract C {\n",
                "    function f() external {\n",
                "        assembly {\n",
                "            for {} 1 {\n",
                "                let x := 1\n",
            ),
            str![[r#"
1:0-6:0 code
2:4-6:0 code
3:17-6:0 code
4:21-6:0 code

"#]],
        ),
        // Lexical fallback recognizes Yul bare blocks after unterminated statements.
        (
            concat!(
                "@ invalid\n",
                "contract C {\n",
                "    function f() external {\n",
                "        assembly {\n",
                "            let x := 1\n",
                "            {\n",
                "                if x {\n",
                "                    pop(x)\n",
                "                }\n",
                "            }\n",
                "        }\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
1:0-12:1 code
2:4-11:5 code
3:17-10:9 code
5:12-9:13 code
6:21-8:17 code

"#]],
        ),
        // Supplements descendants of a recovered unclosed declaration.
        (
            concat!(
                "contract C {\n",
                "    @ invalid\n",
                "    function f() external {\n",
                "    }\n",
            ),
            str![[r#"
0:0-4:0 code
2:4-3:5 code

"#]],
        ),
        // Lexical fallback ignores call options in single-statement control flow.
        (
            concat!(
                "@ invalid\n",
                "contract C {\n",
                "    function f() external {\n",
                "        if (true) this.f{\n",
                "            gas: 1\n",
                "        }();\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
1:0-7:1 code
2:4-6:5 code

"#]],
        ),
        // Folds recovered function-typed variables.
        (
            concat!(
                "@ invalid\n",
                "contract C {\n",
                "    function() external callback = this.f{\n",
                "        gas: 1\n",
                "    };\n",
                "}\n",
            ),
            str![[r#"
1:0-5:1 code
2:4-4:6 code

"#]],
        ),
        // Supplements partial AST with recognized lexical blocks.
        (
            concat!(
                "contract Before {\n",
                "}\n",
                "@ invalid\n",
                "contract After {\n",
                "    function f() external {\n",
                "    }\n",
                "}\n",
            ),
            str![[r#"
0:0-1:1 code
3:0-6:1 code
4:4-5:5 code

"#]],
        ),
        // Preserves AST authority when supplementing parse errors.
        (
            concat!(
                "contract C {\n",
                "    function target() external {}\n",
                "    function f() external {\n",
                "        if (this.target{\n",
                "            gas: 1\n",
                "        }()) {\n",
                "        }\n",
                "    }\n",
                "}\n",
                "@ invalid\n",
                "contract After {\n",
                "}\n",
            ),
            str![[r#"
0:0-8:1 code
2:4-7:5 code
5:13-6:9 code
10:0-11:1 code

"#]],
        ),
        // Ignores call options inside contract headers.
        (
            concat!(
                "contract C layout at this.f{\n",
                "    value: 123\n",
                "}() {\n",
                "}\n",
                "@ invalid\n",
            ),
            str![[r#"
0:0-3:1 code

"#]],
        ),
        // Ignores unclassified braces during lexical fallback.
        (
            concat!(
                "@ invalid\n",
                "import {\n",
                "    A,\n",
                "    B\n",
                "} from \"x.sol\";\n",
                "foo{\n",
                "    value: 1\n",
                "}\n",
                "\"literal { brace }\"; // comment { brace }\n",
            ),
            str![[r#"
1:0-4:15 imports

"#]],
        ),
    ] {
        check(source, expected);
    }
}

#[test]
fn folding_respects_lsp_line_breaks() {
    for newline in ["\n", "\r\n", "\r"] {
        for (source, expected) in [
            (format!("contract C {{{newline}"), "0:0-1:0 code\n"),
            (format!("/*{newline}"), "0:0-1:0 comment\n"),
            (format!("contract C {{{newline}}}{newline}"), "0:0-1:1 code\n"),
            (format!("/* first{newline}second */"), "0:0-1:9 comment\n"),
        ] {
            check(&source, expected);
        }
    }
}

/// Checks the string and rope entry points against the same expected ranges.
fn check(source: &str, expected: impl IntoData) {
    let ranges = folding_ranges(source.to_owned());
    assert_eq!(folding_ranges_from_rope(Rope::from(source)), ranges);
    assert_data_eq!(folding_range_output(&ranges), expected);
}
