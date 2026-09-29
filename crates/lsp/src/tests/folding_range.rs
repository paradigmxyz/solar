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
fn folds_declarations_and_nested_solidity_blocks() {
    check(
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
    );
}

#[test]
fn folds_full_multiline_named_declaration_ranges() {
    check(
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
    );
}

#[test]
fn folds_comments_at_every_nesting_level_and_splits_groups_on_blank_lines() {
    check(
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
    );
}

#[test]
fn folds_import_groups_and_splits_them_on_blank_lines_or_items() {
    check(
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
    );
}

#[test]
fn falls_back_to_import_groups_after_parse_errors() {
    check(
        concat!("@ invalid\n", "import \"a.sol\";\n", "import \"b.sol\";\n",),
        str![[r#"
1:0-2:15 imports

"#]],
    );
}

#[test]
fn lexical_import_fallback_ignores_member_accesses() {
    check(
        concat!("uint256 constant X = Foo.import\n", "    + 1;\n", "@ invalid\n",),
        str![[r#"
0:0-1:8 code

"#]],
    );
}

#[test]
fn extends_recognized_incomplete_blocks_to_physical_eof() {
    check(
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
    );
}

#[test]
fn falls_back_to_recognized_blocks_when_parsing_fails() {
    check(
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
    );
}

#[test]
fn lexical_fallback_recognizes_incomplete_yul_for_post_blocks() {
    check(
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
    );
}

#[test]
fn lexical_fallback_recognizes_yul_bare_blocks_after_unterminated_statements() {
    check(
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
    );
}

#[test]
fn supplements_descendants_of_a_recovered_unclosed_declaration() {
    check(
        concat!("contract C {\n", "    @ invalid\n", "    function f() external {\n", "    }\n",),
        str![[r#"
0:0-4:0 code
2:4-3:5 code

"#]],
    );
}

#[test]
fn lexical_fallback_ignores_call_options_in_single_statement_control_flow() {
    check(
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
    );
}

#[test]
fn lexical_fallback_does_not_treat_function_types_as_declarations() {
    check(
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

"#]],
    );
}

#[test]
fn supplements_partial_ast_with_recognized_lexical_blocks() {
    check(
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
    );
}

#[test]
fn preserves_ast_authority_when_supplementing_parse_errors() {
    check(
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
    );
}

#[test]
fn ignores_call_options_inside_contract_headers() {
    check(
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
    );
}

#[test]
fn folds_yul_declarations_and_nested_bodies() {
    check(
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
    );
}

#[test]
fn ignores_unclassified_braces_during_lexical_fallback() {
    check(
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
    );
}

#[test]
fn uses_utf16_positions_and_crlf_line_endings() {
    check(
        concat!("😀 /* first\r\n", "second */\r\n", "// 一😀\r\n", "// 二😀\r\n",),
        str![[r#"
0:3-1:9 comment
2:0-3:6 comment

"#]],
    );
}

#[test]
fn single_line_sources_have_no_folding_ranges() {
    check("contract C { function f() external {} }", str![""]);
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
