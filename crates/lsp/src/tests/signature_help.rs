use super::{support::signature_help_at, *};
use snapbox::str;

/// Parses `source` as the open `/Signature.sol`.
fn signature_fixture(source: &str) -> RequestFixture {
    RequestFixture::new(&format!("//- /Signature.sol open\n{source}"), "/Signature.sol")
}

fn incomplete_signature_fixture(source: &str) -> RequestFixture {
    RequestFixture::new_allowing_diagnostics(
        &format!("//- /Signature.sol open\n{source}"),
        "/Signature.sol",
    )
}

fn set_source(state: &GlobalState, fixture: &RequestFixture, contents: &str) {
    set_overlay(state, &fixture.project_path("/Signature.sol"), contents, None);
}

#[test]
fn shows_signatures_for_solidity_call_forms() {
    let fixture = signature_fixture(
        r#"
        contract Target {
            constructor(uint256 count, address owner) payable {}
            function set(uint256 value) external payable {}
        }

        contract Plain {
            constructor(uint256 count) {}
        }

        contract Texts {
            function set(string memory text, uint256 value) external pure {}
        }

        contract Paid {
            constructor(uint256 count) payable {}
        }

        contract Base {
            constructor(uint256 baseValue) {}
        }

        interface I {
            event Updated(uint256 value);
            error Failed(address account);
        }

        library Math {
            function bump(uint256 self, uint256 amount) internal pure returns (uint256) {
                return self + amount;
            }
        }

        contract C is Base($1 1) {
            using Math for uint256;

            struct Pair {
                uint256 left;
                address right;
            }

            event Updated(uint256 indexed value);
            error Failed(address account);
            event Declared($27 uint256 value);
            error Rejected($28 address account);

            modifier limited(uint256 limit) {
                _;
            }

            modifier declared($29 uint256 limit) {
                _;
            }

            constructor() limited($2 2) {}

            function add(uint256 lhs, uint256 rhs) public pure returns (uint256) {
                return lhs + rhs;
            }

            function set(uint256 first, uint256 second) public pure {}

            function put(string memory text, uint256 value) public pure {}

            function parse(uint256 value) public pure {}

            function parse(address value) public pure {}

            /// @notice Updates both values.
            /// @dev The values are stored together.
            /// @param first The first value.
            /// @param second The second value.
            function documented(uint256 first, uint256 second) public {}

            function lookup() public pure returns (uint256 result) {
                result = 1;
            }

            function declaredSet($30 uint256 value) public limited(value) {}

            function events(address account) public {
                emit Updated($3 3);
                emit I.Updated($5 1);
                revert Failed($4 account);
            }

            function revertForeign(address account) public pure {
                revert I.Failed($6 account);
            }

            function calls(address account, uint256 value, Target target) public payable {
                add(1, $7 2);
                set({second: $8 2, first: 1});
                set({second /* ignored: colon */: $9 2, first: 1});
                set(($10 1 + 2), 3);
                put(unicode"😀", $11 2);
                parse($12 account);
                new Target($13 1, address(0));
                new Plain{salt: bytes32(0)}($14 1);
                (new Paid){value: 0}($15 1);
                Plain($16 account);
                documented($17 1, 2);
                value.bump($18 Math.bump($19 value, 2));
                target.set{value: 0}($20 1);
                add(1, 2)$21;
                require(true, $22 "failed");
                abi.encode(1, $23 2);
                lookup($24);
                Pair({right: $25 address(0), left: 1});
                new uint256[]($26 value);
            }

            // Member calls require the indexed callsite, so a relative opening-parenthesis offset
            // cannot accidentally pass through the unqualified-name fallback.
            function trivia(Texts target) public pure {
                target.set("", 0);
                target.set("escaped quote: \";,(", $31 1);
                target.set('escaped quote: \';,(', $32 2);
                target.set(unicode"😀; // /*,(", $33 3);
                target.set("value", /* /* ; " ' ( , // */ $34 4);
                target.set("value", // ; " ' ( , /*
                    $35 5);
            }
        }
        "#,
    );

    fixture.check_signature_help(
        &[
            "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11", "$12", "$13",
            "$14", "$15", "$16", "$17", "$18", "$19", "$20", "$21", "$22", "$23", "$24", "$25",
            "$26", "$27", "$28", "$29", "$30", "$31", "$32", "$33", "$34", "$35",
        ],
        str![[r#"
$1:
active signature=Some(0) parameter=Some(0)
constructor(uint256 baseValue)
  12..29
$2:
active signature=Some(0) parameter=Some(0)
modifier limited(uint256 limit)
  17..30
$3:
active signature=Some(0) parameter=Some(0)
event Updated(uint256 indexed value)
  14..35
$4 $6:
active signature=Some(0) parameter=Some(0)
error Failed(address account)
  13..28
$5:
active signature=Some(0) parameter=Some(0)
event Updated(uint256 value)
  14..27
$7:
active signature=Some(0) parameter=Some(1)
function add(uint256 lhs, uint256 rhs) public pure returns (uint256)
  13..24
  26..37
$8 $9:
active signature=Some(0) parameter=Some(1)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42
$10:
active signature=Some(0) parameter=Some(0)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42
$11:
active signature=Some(0) parameter=Some(1)
function put(string memory text, uint256 value) public pure
  13..31
  33..46
$12:
active signature=Some(0) parameter=Some(0)
function parse(address value) public pure
  15..28
function parse(uint256 value) public pure
  15..28
$13:
active signature=Some(0) parameter=Some(0)
constructor(uint256 count, address owner) payable
  12..25
  27..40
$14:
active signature=Some(0) parameter=Some(0)
constructor(uint256 count)
  12..25
$15:
active signature=Some(0) parameter=Some(0)
constructor(uint256 count) payable
  12..25
$16 $21 $27 $28 $29 $30:
<none>
$17:
active signature=Some(0) parameter=Some(0)
function documented(uint256 first, uint256 second) public
  docs=Updates both values. |  | The values are stored together.
  20..33 docs=The first value.
  35..49 docs=The second value.
$18:
active signature=Some(0) parameter=Some(0)
function bump(uint256 amount) internal pure returns (uint256)
  14..28
$19:
active signature=Some(0) parameter=Some(0)
function bump(uint256 self, uint256 amount) internal pure returns (uint256)
  14..26
  28..42
$20:
active signature=Some(0) parameter=Some(0)
function set(uint256 value) external payable
  13..26
$22:
active signature=Some(0) parameter=Some(1)
require(bool, ...)
  8..12
  14..17
$23:
active signature=Some(0) parameter=Some(0)
encode(...) returns (bytes memory)
  7..10
$24:
active signature=Some(0) parameter=None
function lookup() public pure returns (uint256 result)
$25:
active signature=Some(0) parameter=Some(1)
struct Pair(uint256 left, address right)
  12..24
  26..39
$26:
active signature=Some(0) parameter=Some(0)
new uint256[](uint256) returns (uint256[] memory)
  14..21
$31 $32 $33 $34 $35:
active signature=Some(0) parameter=Some(1)
function set(string memory text, uint256 value) external pure
  13..31
  33..46

"#]],
    );
    // Clients without label offsets receive parameter text.
    let information =
        json!({ "documentationFormat": ["markdown"], "activeParameterSupport": true });
    let params = from_json(json!({ "capabilities": { "textDocument": {
        "signatureHelp": { "signatureInformation": information },
    } } }));
    let mut state = fixture.state();
    state.config = Arc::new(negotiate_capabilities(params).1);
    fixture.check_signature_help_in(
        &mut state,
        &["$17"],
        str![[r#"
active signature=Some(0) parameter=Some(0)
function documented(uint256 first, uint256 second) public active=0
  markdown=Updates both values. |  | The values are stored together.
  uint256 first markdown=The first value.
  uint256 second markdown=The second value.

"#]],
    );
}

#[test]
fn selects_events_and_errors_through_import_namespaces() {
    // Reused terminal names make a name-only fallback return both declarations.
    let fixture = RequestFixture::new(
        r#"
        //- /A.sol
        event TransferA(string value);
        error ErrorA(uint8 code);
        event Changed(uint256 amount);
        error Failed(uint256 code);

        //- /B.sol
        import * as A from "./A.sol";

        event TransferB(uint256 value);
        error ErrorB(address account);
        event Changed(address account);
        error Failed(address account);

        contract BContract {
            event TransferC(bytes32 value);
            error ErrorC(bool enabled);
        }

        //- /C.sol open
        import * as A from "./A.sol";
        import * as B from "./B.sol";

        contract C {
            function emits(address account) public {
                emit B.TransferB($1 1);
                emit B.BContract.TransferC($2 bytes32(0));
                emit B.A.TransferA($3 "value");
                emit A.Changed($4 1);
                emit B.Changed($5 account);
            }

            function revertFromModule(address account) public pure {
                revert B.ErrorB($6 account);
            }

            function revertFromContract() public pure {
                revert B.BContract.ErrorC($7 true);
            }

            function revertFromNestedModule() public pure {
                revert B.A.ErrorA($8 1);
            }

            function revertA() public pure {
                revert A.Failed($9 1);
            }

            function revertB(address account) public pure {
                revert B.Failed($10 account);
            }
        }
        "#,
        "/C.sol",
    );

    fixture.check_signature_help(
        &["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10"],
        str![[r#"
$1:
active signature=Some(0) parameter=Some(0)
event TransferB(uint256 value)
  16..29
$2:
active signature=Some(0) parameter=Some(0)
event TransferC(bytes32 value)
  16..29
$3:
active signature=Some(0) parameter=Some(0)
event TransferA(string memory value)
  16..35
$4:
active signature=Some(0) parameter=Some(0)
event Changed(uint256 amount)
  14..28
$5:
active signature=Some(0) parameter=Some(0)
event Changed(address account)
  14..29
$6:
active signature=Some(0) parameter=Some(0)
error ErrorB(address account)
  13..28
$7:
active signature=Some(0) parameter=Some(0)
error ErrorC(bool enabled)
  13..25
$8:
active signature=Some(0) parameter=Some(0)
error ErrorA(uint8 code)
  13..23
$9:
active signature=Some(0) parameter=Some(0)
error Failed(uint256 code)
  13..25
$10:
active signature=Some(0) parameter=Some(0)
error Failed(address account)
  13..28

"#]],
    );
}

#[test]
fn shows_signatures_despite_analysis_errors() {
    let fixture = incomplete_signature_fixture(
        r#"
        contract A {
            function hidden(uint256 value) private pure {}
        }

        contract B {
            struct Callbacks {
                function(uint256 value) internal returns (uint256) callback;
            }

            Callbacks callbacks;

            function set(uint256 first, uint256 second, uint256 third) public pure {}

            function invoke(
                function(uint256 value) external returns (uint256) callback
            ) external returns (uint256) {
                return callback($3 1);
            }

            function use(uint256 value) public returns (uint256) {
                hidden($1 1);
                set([uint256(1), 2].length /* , ignored */, uint256(bytes("a,b").length), $2 3);
                return callbacks.callback($4 value);
            }
        }
        "#,
    );

    fixture.check_signature_help(
        &["$1", "$2", "$3", "$4"],
        str![[r#"
$1:
<none>
$2:
active signature=Some(0) parameter=Some(2)
function set(uint256 first, uint256 second, uint256 third) public pure
  13..26
  28..42
  44..57
$3 $4:
active signature=Some(0) parameter=Some(0)
callback(uint256 value) returns (uint256)
  9..22

"#]],
    );

    // An unclosed call at the end of the file does not hide the selected overload.
    let fixture = incomplete_signature_fixture(
        r#"
        contract First {
            function select(uint256 value) external pure {}
        }

        contract Second {
            function select(address value) external pure {}
        }

        contract C {
            function use(First first) public {
                first.select($1
            }
        }
        "#,
    );
    fixture.check_signature_help(
        &["$1"],
        str![[r#"
active signature=Some(0) parameter=Some(0)
function select(uint256 value) external pure
  16..29

"#]],
    );
}

#[test]
fn checks_stale_call_sites_after_failed_analysis() {
    for (source, replacements, expected) in [
        (
            r#"
            contract C {
                function set(uint256 first, uint256 second) public pure {}

                function use() public pure {
                    set(1, $1 2);
                }
            }
            "#,
            &[("set(1,  2);", "set(1,  2;")][..],
            str![[r#"
active signature=Some(0) parameter=Some(1)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42

"#]],
        ),
        (
            r#"
            contract C {
                function foo(uint256 value) public pure {}
                function bar(address value) public pure {}

                function use() public pure {
                    foo($1 1);
                }
            }
            "#,
            &[("foo( 1);", "bar( 1;")],
            str![[r#"
active signature=Some(0) parameter=Some(0)
function bar(address value) public pure
  13..26

"#]],
        ),
        (
            r#"
            contract C {
                function foo(uint256 value) public pure {}

                function use() public pure {
                    foo($1 1);
                }
            }
            "#,
            &[("function foo(", "function bar(")],
            str![[r#"
<none>

"#]],
        ),
        (
            r#"
            contract A {
                function foo(uint256 value) public pure {}

                function use() public pure {
                    foo($1 1);
                }
            }

            contract B {
                function foo(uint256 value) public pure {}
            }
            "#,
            &[("function foo(", "function bar(")],
            str![[r#"
<none>

"#]],
        ),
        (
            r#"
            contract A {
                function f(uint256 value) external pure {}
            }

            contract B {
                function f(address value) external pure {}
            }

            contract C {
                function use(A a, B b) public pure {
                    a.f($1 1);
                }
            }
            "#,
            &[("a.f( 1);", "b.f( 1;")],
            str![[r#"
<none>

"#]],
        ),
        (
            r#"
            contract A {
                function f(uint8 value) external pure {}
            }

            contract B {
                function f(uint256 value) external pure {}
            }

            contract C {
                function use(A target) public pure {
                    target.f($1 1);
                }
            }
            "#,
            &[("use(A target)", "use(B target)"), ("target.f( 1);", "target.f( 1;")],
            str![[r#"
<none>

"#]],
        ),
    ] {
        let fixture = signature_fixture(source);
        let changed = replacements
            .iter()
            .fold(fixture.project_contents("/Signature.sol"), |contents, (from, to)| {
                contents.replacen(from, to, 1)
            });
        let mut state = fixture.signature_help_state_after_change("/Signature.sol", &changed);
        fixture.check_signature_help_in(&mut state, &["$1"], expected);
    }
}

#[test]
fn pending_calls_resolve_import_aliases() {
    let fixture = RequestFixture::new(
        r#"
        //- /Math.sol
        function twice(uint value) pure returns (uint) { return value * 2; }
        //- /Signature.sol open
        import {twice as double} from "./Math.sol";
        contract C {
            function f() public pure {
                double;$1
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed = fixture.project_contents("/Signature.sol").replace("double;", "double(");
    let mut state = fixture.signature_help_state_after_change("/Signature.sol", &changed);
    fixture.check_signature_help_in(
        &mut state,
        &["$1"],
        str![[r#"
active signature=Some(0) parameter=Some(0)
function twice(uint256 value) internal pure returns (uint256)
  15..28

"#]],
    );
}

#[test]
fn clamps_positions_and_rejects_surrogate_pairs() {
    let fixture = signature_fixture(
        r#"
        contract C {
            function add(uint256 lhs, uint256 rhs) public pure returns (uint256) {
                return lhs + rhs;
            }

            function set(string memory text) public pure {}
            function foo(uint256 lhs, uint256 rhs) public pure {}
            function bar(uint256 first, uint256 second) public pure {}

            function use() public pure {
                add(1,$1
                    2);
                set(unicode"$2😀");
                foo(1,$3 2);
            }
        }
        "#,
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let expected = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(expected.active_parameter, Some(1));
    let clamped = Position::new(position.line, u32::MAX);
    assert_eq!(signature_help_at(&mut state, uri.clone(), clamped), Some(expected));

    let (_, position) = fixture.marker_location("$2");
    assert!(signature_help_at(&mut state, uri.clone(), position).is_some());
    let inside = Position::new(position.line, position.character + 1);
    assert_eq!(signature_help_at(&mut state, uri.clone(), inside), None);

    let (_, position) = fixture.marker_location("$3");
    let original = fixture.project_contents("/Signature.sol");
    let call_start = original.find("foo(1,").unwrap();
    set_source(&state, &fixture, &format!("{}bar(1,", &original[..call_start]));

    // This newly typed callee has no indexed callsite, so signature help resolves its declaration
    // from the cursor's scope in the previous analysis.
    let expected = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(expected.active_parameter, Some(1));
    for position in [Position::new(position.line, u32::MAX), Position::new(u32::MAX, 0)] {
        assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(expected.clone()));
    }
}

#[test]
fn shows_incomplete_calls_on_initial_analysis_and_clamps_lines_to_the_document_end() {
    let fixture = incomplete_signature_fixture(
        r#"
        contract C {
            function target(uint256 amount, address account)
                internal
                view
                returns (uint256)
            {
                return amount + uint256(uint160(account));
            }

            function use() public view returns (uint256) {
                return target(1, $1
        "#,
    );
    fixture.check_signature_help(
        &["$1"],
        str![[r#"
active signature=Some(0) parameter=Some(1)
function target(uint256 amount, address account) internal view returns (uint256)
  16..30
  32..47

"#]],
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let expected = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(expected.active_parameter, Some(1));
    for position in [Position::new(99, 0), Position::new(u32::MAX, u32::MAX)] {
        assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(expected.clone()));
    }
}

#[test]
fn warmed_requests_reject_a_changed_receiver_before_reanalysis() {
    let fixture = signature_fixture(
        r#"
        contract A {
            function f(uint256 value) external pure {}
        }

        contract B {
            function f(uint128 value) external pure {}
        }

        contract C {
            function use(A a, B b) public pure {
                a.f($1 1);
            }
        }
        "#,
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let original = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(original.clone()));

    // Keep the original analysis while an edit changes only the receiver, leaving the terminal
    // name and opening-parenthesis position unchanged.
    let contents = fixture.project_contents("/Signature.sol");
    set_source(&state, &fixture, &contents.replace("a.f(", "b.f("));
    assert_eq!(signature_help_at(&mut state, uri.clone(), position), None);

    // The original callee is still valid at its cached position, but a new call on the
    // same line has a different receiver and must not inherit its signature.
    set_source(&state, &fixture, &contents.replace("a.f(", "a.f(1); b.f("));
    let delta = "a.f(1); ".len() as u32;
    let changed_position = Position::new(position.line, position.character + delta);
    assert_eq!(signature_help_at(&mut state, uri.clone(), changed_position), None);

    set_source(&state, &fixture, &contents);
    assert_eq!(signature_help_at(&mut state, uri, position), Some(original));
}

#[test]
fn warmed_requests_use_current_string_and_comment_boundaries_before_reanalysis() {
    let fixture = signature_fixture(
        r#"
        contract Target {
            function set(bytes memory text, uint256 value) external pure {}
        }

        contract C {
            function use(Target target) public pure {
                target.set(hex"3b3b", 0);
                target.set(hex"3b3b", $1 2);
            }
        }
        "#,
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let original = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(original.active_parameter, Some(1));
    assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(original.clone()));

    let contents = fixture.project_contents("/Signature.sol");
    let start = contents.rfind("hex\"3b3b\",").unwrap();
    let end = start + "hex\"3b3b\",".len();
    // Keep the callsite and cursor positions fixed while introducing invalid or incomplete
    // literals and comments. A comma swallowed by an open token belongs to the first argument.
    for (replacement, active_parameter) in
        [(r#"hex";;,(","#, 1), (r#"hex";;,( ,"#, 0), (r#"/* "; */ ,"#, 1), (r#"/* ";    ,"#, 0)]
    {
        assert_eq!(replacement.len(), end - start);
        let mut changed = contents.clone();
        changed.replace_range(start..end, replacement);
        set_source(&state, &fixture, &changed);
        let mut expected = original.clone();
        expected.active_parameter = Some(active_parameter);
        for _ in 0..2 {
            let help = signature_help_at(&mut state, uri.clone(), position);
            assert_eq!(help, Some(expected.clone()));
        }
    }

    set_source(&state, &fixture, &contents);
    assert_eq!(signature_help_at(&mut state, uri, position), Some(original));
}

#[test]
fn warmed_requests_use_changed_lines_and_utf16_columns_before_reanalysis() {
    let fixture = signature_fixture(
        r#"
        contract C {
            function set(string memory text, uint256 value) public pure {}

            function use() public pure {
                set(unicode"😀", $1 2);

            }
        }
        "#,
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let original = signature_help_at(&mut state, uri.clone(), position).unwrap();
    assert_eq!(original.active_parameter, Some(1));
    assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(original.clone()));

    // Move the call onto a new line with changed UTF-16 columns and CRLF endings while keeping
    // the previous analysis. The lexical fallback must use the current document.
    let changed = fixture
        .project_contents("/Signature.sol")
        .replace("set(unicode\"😀\",", "\n        /* 😀 */ set(unicode\"😀中\",")
        .replace('\n', "\r\n");
    let prefix = &changed[..changed.find(" 2);").unwrap()];
    let position = Position::new(
        prefix.bytes().filter(|&byte| byte == b'\n').count() as u32,
        prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
    );
    set_source(&state, &fixture, &changed);
    for _ in 0..2 {
        assert_eq!(signature_help_at(&mut state, uri.clone(), position), Some(original.clone()));
    }
}

#[test]
fn warmed_member_signature_help_survives_an_earlier_line_edit() {
    let fixture = signature_fixture(
        r#"
        contract Target {
            function set(uint256 value) external pure {}
        }

        contract C {
            function use(Target target) public pure { // short
                /* 😀 */ target.set($1 2);
            }
        }
        "#,
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let expected = signature_help_at(&mut state, uri.clone(), position).unwrap();

    let original = fixture.project_contents("/Signature.sol");
    // Preserve the analysis while changing byte offsets before the call. Its opening
    // delimiter and callee retain their LSP positions, including the UTF-16 column.
    for changed in [
        original.replace("// short", "// this comment is now much longer"),
        original.replace("// short", "// 😀"),
        original.replace('\n', "\r\n"),
        original.clone(),
    ] {
        set_source(&state, &fixture, &changed);
        for _ in 0..2 {
            let help = signature_help_at(&mut state, uri.clone(), position);
            assert_eq!(help, Some(expected.clone()));
        }
    }
}
