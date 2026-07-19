use super::support::RequestFixture;
use snapbox::str;

#[test]
fn resolves_function_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 stateValue;

            function target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public view {
                uint256 callerLocal = $1target(stateValue);
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Symbols.sol:2:13 function target(uint256 input) public view returns (uint256 output) {

"#]],
    );
}

#[test]
fn resolves_member_targets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { $3A, B }
            struct Data { uint256 field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.$1A;
                return data.$2field;
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Members.sol:1:18 enum Choice { A, B }

"#]],
    );
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Members.sol:2:26 struct Data { uint256 field; }

"#]],
    );
    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Members.sol:1:18 enum Choice { A, B }

"#]],
    );
}

#[test]
fn resolves_overloaded_calls_to_selected_definition() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            function f(uint256) public {}
            function f(string memory) public {}
            function g() public {
                $1f(uint256(1));
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Overload.sol:1:13 function f(uint256) public {}

"#]],
    );
}

#[test]
fn resolves_using_directives_and_attached_functions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Using.sol
        library L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using $1L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.$2inc();
            }
        }
        "#,
        "/Using.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Using.sol:0:8 library L {

"#]],
    );
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Using.sol:1:13 function inc(uint256 value) internal pure returns (uint256) {

"#]],
    );
}

#[test]
fn distinguishes_function_declarations_from_definitions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Navigation.sol
        interface I {
            function $1f() external returns (uint256);
        }
        "#,
        "/Navigation.sol",
    );

    fixture.check_goto_declaration(
        "$1",
        str![[r#"
/Navigation.sol:1:13 function f() external returns (uint256);

"#]],
    );
    fixture.check_goto_definition(
        "$1",
        str![[r#"
<none>

"#]],
    );
}
