use super::support::RequestFixture;
use lsp_types::Documentation;
use snapbox::str;

#[test]
fn shows_function_signature_and_active_parameter() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function add(uint256 lhs, uint256 rhs) public pure returns (uint256) {
                return lhs + rhs;
            }

            function use() public pure {
                add(1, $1 2);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
function add(uint256 lhs, uint256 rhs) public pure returns (uint256)
  13..24
  26..37

"#]],
    );
}

#[test]
fn uses_parameter_text_when_the_client_does_not_support_label_offsets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        function add(uint256 lhs, uint256 rhs) pure returns (uint256) {
            return lhs + rhs;
        }

        function use() pure {
            add($1 1, 2);
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help_without_label_offsets(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function add(uint256 lhs, uint256 rhs) internal pure returns (uint256)
  uint256 lhs
  uint256 rhs

"#]],
    );
}

#[test]
fn maps_named_arguments_to_declared_parameters() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function set(uint256 first, uint256 second) public pure {}

            function use() public pure {
                set({second: $1 2, first: 1});
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42

"#]],
    );
}

#[test]
fn maps_named_arguments_when_comments_contain_colons() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function set(uint256 first, uint256 second) public pure {}

            function use() public pure {
                set({second /* ignored: colon */: $1 2, first: 1});
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42

"#]],
    );
}

#[test]
fn does_not_show_signature_help_in_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            event Updated($1 uint256 value);
            error Failed($2 address account);

            modifier limited($3 uint256 limit) {
                _;
            }

            function set($4 uint256 value) public limited(value) {}
        }
        "#,
        "/Signature.sol",
    );

    for marker in ["$1", "$2", "$3", "$4"] {
        fixture.check_signature_help(marker, "<none>\n");
    }
}

#[test]
fn keeps_help_for_an_unclosed_call_after_failed_analysis() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function set(uint256 first, uint256 second) public pure {}

            function use() public pure {
                set(1, $1 2);
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed = fixture.project_contents("/Signature.sol").replace("set(1,  2);", "set(1,  2;");

    fixture.check_signature_help_after_change(
        "$1",
        "/Signature.sol",
        &changed,
        str![[r#"
active signature=Some(0) parameter=Some(1)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42

"#]],
    );
}

#[test]
fn shows_help_for_an_incomplete_call_on_initial_analysis() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
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
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
function target(uint256 amount, address account) internal view returns (uint256)
  16..30
  32..47

"#]],
    );
}

#[test]
fn resolves_an_incomplete_member_call_on_initial_analysis() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
        contract A {
            function select(uint256 value) external pure {}
        }

        contract B {
            function select(address value) external pure {}
        }

        contract C {
            function use(A a) public {
                a.select($1
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function select(uint256 value) external pure
  16..29

"#]],
    );
}

#[test]
fn does_not_fallback_to_an_inaccessible_private_function() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
        contract A {
            function hidden(uint256 value) private pure {}
        }

        contract B {
            function use() public pure {
                hidden($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help("$1", "<none>\n");
}

#[test]
fn does_not_reuse_a_stale_call_site_after_the_callee_changes() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function foo(uint256 value) public pure {}
            function bar(address value) public pure {}

            function use() public pure {
                foo($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed = fixture.project_contents("/Signature.sol").replace("foo( 1);", "bar( 1;");

    fixture.check_signature_help_after_change(
        "$1",
        "/Signature.sol",
        &changed,
        str![[r#"
active signature=Some(0) parameter=Some(0)
function bar(address value) public pure
  13..26

"#]],
    );
}

#[test]
fn does_not_reuse_a_stale_signature_after_the_declaration_changes() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function foo(uint256 value) public pure {}

            function use() public pure {
                foo($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed =
        fixture.project_contents("/Signature.sol").replace("function foo(", "function bar(");

    fixture.check_signature_help_after_change("$1", "/Signature.sol", &changed, "<none>\n");
}

#[test]
fn does_not_reuse_a_stale_signature_when_an_identical_declaration_still_exists() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
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
        "/Signature.sol",
    );
    let changed =
        fixture.project_contents("/Signature.sol").replacen("function foo(", "function bar(", 1);

    fixture.check_signature_help_after_change("$1", "/Signature.sol", &changed, "<none>\n");
}

#[test]
fn does_not_reuse_a_stale_member_call_after_the_receiver_changes() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract A {
            function f(uint256 value) external pure {}
        }

        contract B {
            function f(address value) external pure {}
        }

        contract C {
            function use(A a, B b) public {
                a.f($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed = fixture.project_contents("/Signature.sol").replace("a.f( 1);", "b.f( 1;");

    fixture.check_signature_help_after_change("$1", "/Signature.sol", &changed, "<none>\n");
}

#[test]
fn does_not_reuse_a_stale_member_call_after_the_receiver_type_changes() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract A {
            function f(uint8 value) external pure {}
        }

        contract B {
            function f(uint256 value) external pure {}
        }

        contract C {
            function use(A target) public {
                target.f($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );
    let changed = fixture
        .project_contents("/Signature.sol")
        .replace("use(A target)", "use(B target)")
        .replace("target.f( 1);", "target.f( 1;");

    fixture.check_signature_help_after_change("$1", "/Signature.sol", &changed, "<none>\n");
}

#[test]
fn puts_the_type_checked_overload_first() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function parse(uint256 value) public pure {}
            function parse(address value) public pure {}

            function use(address account) public pure {
                parse($1 account);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function parse(address value) public pure
  15..28
function parse(uint256 value) public pure
  15..28

"#]],
    );
}

#[test]
fn shows_contract_constructor_signatures() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Target {
            constructor(uint256 count, address owner) payable {}
        }

        contract C {
            function deploy() public {
                new Target($1 1, address(0));
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
constructor(uint256 count, address owner) payable
  12..25
  27..40

"#]],
    );
}

#[test]
fn shows_constructor_signatures_with_create_options() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Target {
            constructor(uint256 count) {}
        }

        contract C {
            function deploy(bytes32 salt) public returns (Target) {
                return new Target{salt: salt}($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
constructor(uint256 count)
  12..25

"#]],
    );
}

#[test]
fn shows_constructor_signatures_for_parenthesized_creation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Target {
            constructor(uint256 count) payable {}
        }

        contract C {
            function deploy() public returns (Target) {
                return (new Target){value: 0}($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
constructor(uint256 count) payable
  12..25

"#]],
    );
}

#[test]
fn does_not_use_the_constructor_signature_for_a_contract_cast() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Target {
            constructor(uint256 count) {}
        }

        contract C {
            function cast(address account) public pure returns (Target) {
                return Target($1 account);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help("$1", "<none>\n");
}

#[test]
fn includes_resolved_natspec_documentation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            /// @notice Updates both values.
            /// @dev The values are stored together.
            /// @param first The first value.
            /// @param second The second value.
            function set(uint256 first, uint256 second) public {}

            function use() public {
                set($1 1, 2);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function set(uint256 first, uint256 second) public
  docs=Updates both values. |  | The values are stored together.
  13..26 docs=The first value.
  28..42 docs=The second value.

"#]],
    );
}

#[test]
fn respects_optional_signature_information_capabilities() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            /// @notice Updates the value.
            /// @param value The new value.
            function set(uint256 value) public {}

            function use() public {
                set($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    let help = fixture.signature_help_response("$1").unwrap();
    let signature = &help.signatures[0];
    assert!(matches!(signature.documentation, Some(Documentation::String(_))));
    assert!(matches!(
        signature.parameters.as_ref().unwrap()[0].documentation,
        Some(Documentation::String(_))
    ));
    assert_eq!(signature.active_parameter, None);
    assert_eq!(help.active_parameter, Some(0));
}

#[test]
fn supports_special_solidity_callable_forms() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Base {
            constructor(uint256 baseValue) {}
        }

        contract C is Base($1 1) {
            event Updated(uint256 indexed value);
            error Failed(address account);

            modifier limited(uint256 limit) {
                _;
            }

            constructor() limited($2 2) {}

            function use(address account) public {
                emit Updated($3 3);
                revert Failed($4 account);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
constructor(uint256 baseValue)
  12..29

"#]],
    );
    fixture.check_signature_help(
        "$2",
        str![[r#"
active signature=Some(0) parameter=Some(0)
modifier limited(uint256 limit)
  17..30

"#]],
    );
    fixture.check_signature_help(
        "$3",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event Updated(uint256 indexed value)
  14..35

"#]],
    );
    fixture.check_signature_help(
        "$4",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error Failed(address account)
  13..28

"#]],
    );
}

#[test]
fn supports_qualified_foreign_events_and_errors() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        interface I {
            event Updated(uint256 value);
            error Failed(address account);
        }

        contract C {
            function emitForeign() public {
                emit I.Updated($1 1);
            }

            function revertForeign(address account) public pure {
                revert I.Failed($2 account);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event Updated(uint256 value)
  14..27

"#]],
    );
    fixture.check_signature_help(
        "$2",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error Failed(address account)
  13..28

"#]],
    );
}

#[test]
fn supports_events_and_errors_through_import_namespaces() {
    let fixture = RequestFixture::new(
        r#"
        //- /A.sol
        event TransferA(string value);
        error ErrorA(uint8 code);

        //- /B.sol
        import * as A from "./A.sol";

        event TransferB(uint256 value);
        error ErrorB(address account);

        contract BContract {
            event TransferC(bytes32 value);
            error ErrorC(bool enabled);
        }

        //- /C.sol open
        import * as B from "./B.sol";

        contract C {
            function emitFromModule() public {
                emit B.TransferB($1 1);
            }

            function emitFromContract() public {
                emit B.BContract.TransferC($2 bytes32(0));
            }

            function emitFromNestedModule() public {
                emit B.A.TransferA($3 "value");
            }

            function revertFromModule(address account) public pure {
                revert B.ErrorB($4 account);
            }

            function revertFromContract() public pure {
                revert B.BContract.ErrorC($5 true);
            }

            function revertFromNestedModule() public pure {
                revert B.A.ErrorA($6 1);
            }
        }
        "#,
        "/C.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event TransferB(uint256 value)
  16..29

"#]],
    );
    fixture.check_signature_help(
        "$2",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event TransferC(bytes32 value)
  16..29

"#]],
    );
    fixture.check_signature_help(
        "$3",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event TransferA(string memory value)
  16..35

"#]],
    );
    fixture.check_signature_help(
        "$4",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error ErrorB(address account)
  13..28

"#]],
    );
    fixture.check_signature_help(
        "$5",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error ErrorC(bool enabled)
  13..25

"#]],
    );
    fixture.check_signature_help(
        "$6",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error ErrorA(uint8 code)
  13..23

"#]],
    );
}

#[test]
fn selects_the_exact_qualified_event_and_error_callsite() {
    // Reused terminal names make a name-only fallback return both declarations.
    let fixture = RequestFixture::new(
        r#"
        //- /A.sol
        event Changed(uint256 amount);
        error Failed(uint256 code);

        //- /B.sol
        event Changed(address account);
        error Failed(address account);

        //- /C.sol open
        import * as A from "./A.sol";
        import * as B from "./B.sol";

        contract C {
            function emitA() public {
                emit A.Changed($1 1);
            }

            function emitB(address account) public {
                emit B.Changed($2 account);
            }

            function revertA() public pure {
                revert A.Failed($3 1);
            }

            function revertB(address account) public pure {
                revert B.Failed($4 account);
            }
        }
        "#,
        "/C.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event Changed(uint256 amount)
  14..28

"#]],
    );
    fixture.check_signature_help(
        "$2",
        str![[r#"
active signature=Some(0) parameter=Some(0)
event Changed(address account)
  14..29

"#]],
    );
    fixture.check_signature_help(
        "$3",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error Failed(uint256 code)
  13..25

"#]],
    );
    fixture.check_signature_help(
        "$4",
        str![[r#"
active signature=Some(0) parameter=Some(0)
error Failed(address account)
  13..28

"#]],
    );
}

#[test]
fn hides_the_receiver_for_attached_library_functions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        library Math {
            function bump(uint256 self, uint256 amount) internal pure returns (uint256) {
                return self + amount;
            }
        }

        contract C {
            using Math for uint256;

            function use(uint256 value) public pure returns (uint256) {
                return value.bump($1 2);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function bump(uint256 amount) internal pure returns (uint256)
  14..28

"#]],
    );
}

#[test]
fn supports_function_call_options() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract Target {
            function set(uint256 value) external payable {}
        }

        contract C {
            function use(Target target) public {
                target.set{value: 0}($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function set(uint256 value) external payable
  13..26

"#]],
    );
}

#[test]
fn counts_only_commas_directly_in_the_selected_call() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
        contract C {
            function set(uint256 first, uint256 second, uint256 third) public pure {}

            function use() public pure {
                set([uint256(1), 2].length /* , ignored */, uint256(bytes("a,b").length), $1 3);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(2)
function set(uint256 first, uint256 second, uint256 third) public pure
  13..26
  28..42
  44..57

"#]],
    );
}

#[test]
fn finds_the_outer_call_inside_a_grouped_argument() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function set(uint256 first, uint256 second) public pure {}

            function use() public pure {
                set(($1 1 + 2), 3);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
function set(uint256 first, uint256 second) public pure
  13..26
  28..42

"#]],
    );
}

#[test]
fn does_not_show_the_current_signature_on_its_closing_parenthesis() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        function set(uint256 value) pure {}
        function use() pure {
            set(1)$1;
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help("$1", "<none>\n");
}

#[test]
fn handles_utf16_positions_before_the_active_argument() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function set(string memory text, uint256 value) public pure {}

            function use() public pure {
                set(unicode"😀", $1 2);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
function set(string memory text, uint256 value) public pure
  13..31
  33..46

"#]],
    );
}

#[test]
fn supports_function_typed_variables() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
        contract C {
            function invoke(
                function(uint256 value) external returns (uint256) callback
            ) external returns (uint256) {
                return callback($1 1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
callback(uint256 value) returns (uint256)
  9..22

"#]],
    );
}

#[test]
fn supports_variadic_builtins() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        function check() pure {
            require(true, $1 "failed");
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
require(bool, ...)
  8..12
  14..17

"#]],
    );
}

#[test]
fn keeps_the_variadic_parameter_active_for_later_arguments() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        function encode() pure returns (bytes memory) {
            return abi.encode(1, $1 2);
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
encode(...) returns (bytes memory)
  7..10

"#]],
    );
}

#[test]
fn includes_named_return_parameters() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function lookup() public pure returns (uint256 result) {
                result = 1;
            }

            function use() public pure {
                lookup($1);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=None
function lookup() public pure returns (uint256 result)

"#]],
    );
}

#[test]
fn supports_named_struct_construction() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            struct Pair {
                uint256 left;
                address right;
            }

            function make() public pure returns (Pair memory) {
                return Pair({right: $1 address(0), left: 1});
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(1)
struct Pair(uint256 left, address right)
  12..24
  26..39

"#]],
    );
}

#[test]
fn supports_dynamic_array_allocation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Signature.sol open
        contract C {
            function make(uint256 length) public pure returns (uint256[] memory) {
                return new uint256[]($1 length);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
new uint256[](uint256) returns (uint256[] memory)
  14..21

"#]],
    );
}

#[test]
fn supports_function_typed_struct_fields() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Signature.sol open
        contract C {
            struct Callbacks {
                function(uint256 value) internal returns (uint256) callback;
            }

            Callbacks callbacks;

            function use(uint256 value) public returns (uint256) {
                return callbacks.callback($1 value);
            }
        }
        "#,
        "/Signature.sol",
    );

    fixture.check_signature_help(
        "$1",
        str![[r#"
active signature=Some(0) parameter=Some(0)
callback(uint256 value) returns (uint256)
  9..22

"#]],
    );
}
