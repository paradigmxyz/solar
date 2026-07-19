use super::support::RequestFixture;
use snapbox::str;

#[test]
fn indexes_function_and_state_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 $2stateValue;

            function $1target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public view {
                uint256 callerLocal = target(stateValue);
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Symbols.sol:2:13 function target(uint256 input) public view returns (uint256 output) {
/Symbols.sol:7:30 uint256 callerLocal = target(stateValue);

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Symbols.sol:1:12 uint256 stateValue;
/Symbols.sol:3:37 uint256 localValue = input + stateValue;
/Symbols.sol:7:37 uint256 callerLocal = target(stateValue);

"#]],
    );
}

#[test]
fn indexes_member_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { A, B }
            struct Data { uint256 $1field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.A;
                return data.field;
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Members.sol:2:26 struct Data { uint256 field; }
/Members.sol:5:20 return data.field;

"#]],
    );
}

#[test]
fn distinguishes_enum_and_variant_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Enum.sol
        contract C {
            enum $1Choice { $2A, B }

            function read() public pure returns (Choice) {
                return Choice.A;
            }
        }
        "#,
        "/Enum.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Enum.sol:1:9 enum Choice { A, B }
/Enum.sol:2:41 function read() public pure returns (Choice) {
/Enum.sol:3:15 return Choice.A;

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Enum.sol:1:18 enum Choice { A, B }
/Enum.sol:3:22 return Choice.A;

"#]],
    );
}

#[test]
fn skips_generated_getter_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Getter.sol
        contract C {
            uint256 public $1x;

            function read() external view returns (uint256) {
                return x;
            }
        }
        "#,
        "/Getter.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Getter.sol:1:19 uint256 public x;
/Getter.sol:3:15 return x;

"#]],
    );
}

#[test]
fn indexes_selected_overload_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            function $1f(uint256) public {}
            function $2f(string memory) public {}
            function g() public {
                f(uint256(1));
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Overload.sol:1:13 function f(uint256) public {}
/Overload.sol:4:8 f(uint256(1));

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Overload.sol:2:13 function f(string memory) public {}

"#]],
    );
}

#[test]
fn indexes_using_directive_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Using.sol
        library $1L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.inc();
            }
        }
        "#,
        "/Using.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Using.sol:0:8 library L {
/Using.sol:5:6 using L for uint256;

"#]],
    );
}
