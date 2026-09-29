use super::*;
use snapbox::str;

#[test]
fn selects_expressions_declarations_and_yul_in_request_order() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selection.sol open
        contract C {
            struct User { uint256 balance; }
            User[] users;

            function f(uint256 val$1ue, address other) external view returns (uint256) {
                return (val$2ue + 1) * users[val$3ue].bal$4ance;
            }

            function g() external pure {
                assembly {
                    let value := 2
                    let result := add(val$5ue, 1)
                }
            }
        }
        "#,
        "/Selection.sol",
    );

    fixture.check_selection_ranges(
        &["$1", "$4", "$3", "$2", "$5"],
        str![[r#"
0:
  3:23-3:28
  3:15-3:28
  3:14-3:44
  3:4-5:5
  0:0-12:1
1:
  4:42-4:49
  4:29-4:49
  4:15-4:49
  4:8-4:50
  3:77-5:5
  3:4-5:5
  0:0-12:1
2:
  4:35-4:40
  4:29-4:41
  4:29-4:49
  4:15-4:49
  4:8-4:50
  3:77-5:5
  3:4-5:5
  0:0-12:1
3:
  4:16-4:21
  4:16-4:25
  4:15-4:26
  4:15-4:49
  4:8-4:50
  3:77-5:5
  3:4-5:5
  0:0-12:1
4:
  9:30-9:35
  9:26-9:39
  9:12-9:39
  7:17-10:9
  7:8-10:9
  6:31-11:5
  6:4-11:5
  0:0-12:1

"#]],
    );
}

#[test]
fn uses_cached_utf16_positions_for_crlf_documents() {
    let fixture = RequestFixture::new(
        concat!(
            "//- /Open.sol open\r\n",
            "contract C {\r\n",
            "    function f(uint256 value) external pure returns (uint256) {\r\n",
            "        /* 中😀 */ return val$1ue;\r\n",
            "    }\r\n",
            "}\r\n",
            "//- /Disk.sol\r\n",
            "contract C {\r\n",
            "    function f(uint256 value) external pure returns (uint256) {\r\n",
            "        /* 中😀 */ return val$2ue;\r\n",
            "    }\r\n",
            "}",
        ),
        "/Open.sol",
    );

    fixture.check_selection_ranges(
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
    let mut state = fixture.state();
    let first = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let cached = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let disk = fixture.selection_range_response_in_state(&mut state, &["$2"]);
    assert_eq!(cached, first);
    assert_eq!(disk, first);

    let valid = Position::new(2, 28);
    for invalid in [Position::new(99, 0), Position::new(2, 13)] {
        fixture.check_selection_range_error(
            "/Open.sol",
            vec![valid, invalid],
            ErrorCode::INVALID_PARAMS,
        );
    }
}

#[test]
fn clamps_positions_and_supports_standalone_carriage_returns() {
    let fixture = RequestFixture::new(
        concat!(
            "//- /Clamp.sol open\n",
            "contract C {}\n",
            "//- /CarriageReturn.sol open\n",
            "contract C {}\rcontract D {}",
        ),
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
fn falls_back_to_cursor_and_document_outside_syntax() {
    let fixture = RequestFixture::new(
        r#"
        //- /Fallback.sol open
        // com$1ment
        $2
        contract C {}$3
        contract D {}

        //- /Empty.sol open
        $4
        "#,
        "/Empty.sol",
    );

    // Non-empty range ends are exclusive, so `$3` is outside the first contract.
    fixture.check_selection_ranges(
        &["$2", "$1", "$3"],
        str![[r#"
0:
  1:0-1:0
  0:0-3:13
1:
  0:6-0:6
  0:0-3:13
2:
  2:13-2:13
  0:0-3:13

"#]],
    );
    fixture.check_selection_ranges(
        &["$4"],
        str![[r#"
0:
  0:0-0:0

"#]],
    );
}

#[test]
fn prefers_open_vfs_contents_and_reads_closed_documents_from_disk() {
    let fixture = RequestFixture::new(
        r#"
        //- /Open.sol open
        contract Op$1en {}

        //- /Disk.sol
        contract Di$2sk {}
        "#,
        "/Open.sol",
    );
    fixture.write_file("/Open.sol", "contract DiskVersion {}");

    for marker in ["$1", "$2"] {
        fixture.check_selection_ranges(
            &[marker],
            str![[r#"
0:
  0:9-0:13
  0:0-0:16

"#]],
        );
    }
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
        //- /Open.sol open
        @$1
        //- /Disk.sol
        @$2
        "#,
        "/Open.sol",
    );

    fixture.check_selection_ranges(
        &["$1"],
        str![[r#"
0:
  0:1-0:1
  0:0-0:1

"#]],
    );
    let mut state = fixture.state();
    let first = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let cached = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let disk = fixture.selection_range_response_in_state(&mut state, &["$2"]);
    assert_eq!(cached, first);
    assert_eq!(disk, first);
}

#[test]
fn content_changes_replace_cached_selection_ranges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selection.sol open
        contract $1A {}
        "#,
        "/Selection.sol",
    );
    let mut state = fixture.state();
    let first = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let (_, position) = fixture.marker_location("$1");
    let changed_source = "contract LongName {}";
    set_overlay(&state, &fixture.project_path("/Selection.sol"), changed_source, 2);

    let changed = fixture.selection_range_response_in_state(&mut state, &["$1"]);
    let expected = crate::selection_range::selection_ranges(changed_source.into(), &[position])
        .expect("the unchanged request position should remain valid");

    assert_ne!(changed, first);
    assert_eq!(changed, expected);
}
