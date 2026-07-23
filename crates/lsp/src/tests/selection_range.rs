use super::support::RequestFixture;
use async_lsp::ErrorCode;
use lsp_types::Position;
use snapbox::str;

#[test]
fn selects_nested_expressions_and_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selection.sol open
        contract C {
            function f(uint256 value) external pure returns (uint256) {
                return (val$1ue + 1) * 2;
            }
        }
        "#,
        "/Selection.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  2:16-2:21
  2:16-2:25
  2:15-2:26
  2:15-2:30
  2:8-2:31
  1:62-3:5
  1:4-3:5
  0:0-4:1

"#]],
    );
}

#[test]
fn selects_parameters_without_a_function_header_range() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selection.sol open
        contract C {
            function f(uint256 val$1ue, address other) external {}
        }
        "#,
        "/Selection.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  1:23-1:28
  1:15-1:28
  1:14-1:44
  1:4-1:56
  0:0-2:1

"#]],
    );
}

#[test]
fn preserves_request_order_for_member_and_index_expressions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selection.sol open
        contract C {
            struct User { uint256 balance; }
            User[] users;

            function f(uint256 index) external view returns (uint256) {
                return users[ind$1ex].bal$2ance;
            }
        }
        "#,
        "/Selection.sol",
    );

    fixture.check_selection_ranges(
        &["$2", "$1"],
        str![[r#"
0:
  4:28-4:35
  4:15-4:35
  4:8-4:36
  3:62-5:5
  3:4-5:5
  0:0-6:1
1:
  4:21-4:26
  4:15-4:27
  4:15-4:35
  4:8-4:36
  3:62-5:5
  3:4-5:5
  0:0-6:1

"#]],
    );
}

#[test]
fn uses_utf16_positions_for_crlf_documents_without_waiting_for_analysis() {
    let fixture = RequestFixture::new(
        concat!(
            "//- /Unicode.sol open\r\n",
            "contract C {\r\n",
            "    function f(uint256 value) external pure returns (uint256) {\r\n",
            "        /* 中😀 */ return val$1ue;\r\n",
            "    }\r\n",
            "}",
        ),
        "/Unicode.sol",
    );

    fixture.check_selection_ranges_while_analysis_pending(
        &["$1"],
        str![[r#"
0:
  2:25-2:30
  2:18-2:31
  1:62-3:5
  1:4-3:5
  0:0-4:1

"#]],
    );
}

#[test]
fn rejects_invalid_utf16_positions() {
    let fixture = RequestFixture::new(
        concat!(
            "//- /Unicode.sol open\r\n",
            "contract C {\r\n",
            "    function f(uint256 value) external pure returns (uint256) {\r\n",
            "        /* 中😀 */ return value;\r\n",
            "    }\r\n",
            "}",
        ),
        "/Unicode.sol",
    );
    let valid = Position::new(2, 28);

    for invalid in [Position::new(99, 0), Position::new(2, 13)] {
        fixture.check_selection_range_error(
            "/Unicode.sol",
            vec![valid, invalid],
            ErrorCode::INVALID_PARAMS,
        );
    }
}

#[test]
fn clamps_positions_past_the_line_end() {
    let fixture = RequestFixture::new(
        r#"
        //- /Clamp.sol open
        contract C {}
        "#,
        "/Clamp.sol",
    );

    fixture.check_selection_ranges_at(
        "/Clamp.sol",
        vec![Position::new(0, u32::MAX)],
        &[Position::new(0, 13)],
        str![[r#"
0:
  0:13-0:13
  0:0-0:13

"#]],
    );
}

#[test]
fn supports_standalone_carriage_return_line_endings() {
    let fixture = RequestFixture::new(
        concat!("//- /CarriageReturn.sol open\n", "contract C {}\rcontract D {}"),
        "/CarriageReturn.sol",
    );

    fixture.check_selection_ranges_at(
        "/CarriageReturn.sol",
        vec![Position::new(1, 9)],
        &[Position::new(1, 9)],
        str![[r#"
0:
  1:9-1:10
  1:0-1:13
  0:0-1:13

"#]],
    );
}

#[test]
fn treats_non_empty_range_ends_as_exclusive() {
    let fixture = RequestFixture::new(
        r#"
        //- /ExclusiveEnd.sol open
        contract C {}$1
        contract D {}
        "#,
        "/ExclusiveEnd.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  0:13-0:13
  0:0-1:13

"#]],
    );
}

#[test]
fn parses_selection_ranges_on_the_blocking_pool() {
    let fixture = RequestFixture::new(
        r#"
        //- /Blocking.sol open
        contract Bl$1ocking {}
        "#,
        "/Blocking.sol",
    );

    fixture.check_selection_range_uses_blocking_pool(
        &["$1"],
        str![[r#"
0:
  0:9-0:17
  0:0-0:20

"#]],
    );
}

#[test]
fn falls_back_to_cursor_and_document_for_comments_and_whitespace() {
    let fixture = RequestFixture::new(
        r#"
        //- /Fallback.sol open
        // com$1ment
        $2
        contract C {}
        "#,
        "/Fallback.sol",
    );

    fixture.check_selection_ranges(
        &["$2", "$1"],
        str![[r#"
0:
  1:0-1:0
  0:0-2:13
1:
  0:6-0:6
  0:0-2:13

"#]],
    );
}

#[test]
fn returns_one_empty_range_for_an_empty_document() {
    let fixture = RequestFixture::new(
        r#"
        //- /Empty.sol open
        $1
        "#,
        "/Empty.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  0:0-0:0

"#]],
    );
}

#[test]
fn prefers_open_vfs_contents_over_stale_disk() {
    let fixture = RequestFixture::new(
        r#"
        //- /Open.sol open
        contract Op$1en {}
        "#,
        "/Open.sol",
    );
    fixture.write_file("/Open.sol", "contract DiskVersion {}");

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  0:9-0:13
  0:0-0:16

"#]],
    );
}

#[test]
fn reads_closed_documents_from_disk() {
    let fixture = RequestFixture::new(
        r#"
        //- /Disk.sol
        contract Di$1sk {}
        "#,
        "/Disk.sol",
    );

    fixture.check_selection_ranges_from_disk(
        &["$1"],
        str![[r#"
0:
  0:9-0:13
  0:0-0:16

"#]],
    );
}

#[test]
fn recovers_selection_ranges_from_incomplete_source() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Incomplete.sol open
        contract C {
            function f(uint256 value) external pure returns (uint256) {
                return val$1ue
        "#,
        "/Incomplete.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  2:15-2:20
  2:8-2:20
  1:62-2:20
  1:4-2:20
  0:0-2:20

"#]],
    );
}

#[test]
fn falls_back_when_source_cannot_be_parsed() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Invalid.sol open
        @$1
        "#,
        "/Invalid.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  0:1-0:1
  0:0-0:1

"#]],
    );
}

#[test]
fn selects_inline_yul_expressions_and_statements() {
    let fixture = RequestFixture::new(
        r#"
        //- /Yul.sol open
        contract C {
            function f() external pure {
                assembly {
                    let value := 2
                    let result := add(val$1ue, 1)
                }
            }
        }
        "#,
        "/Yul.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  4:30-4:35
  4:26-4:39
  4:12-4:39
  2:17-5:9
  2:8-5:9
  1:31-6:5
  1:4-6:5
  0:0-7:1

"#]],
    );
}
