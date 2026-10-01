use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn resolves_definitions_and_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Navigation.sol
        library L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using $1L for uint256;

        interface I {
            function $2f() external returns (uint256);
        }

        contract Base {
            /// @param $3amount The amount.
            function g(uint amount) public virtual {}
        }

        contract C is Base {
            enum $14Choice { $4A, B }
            struct Data { uint256 field; }
            uint256 public stateValue;

            function g(uint $5value) public override {}

            function target(uint256 input) public view returns (uint256 output) {
                output = input + $6stateValue;
            }

            function pick(uint256) public {}
            function $7pick(string memory) public {}

            function caller(Data memory data) public returns ($8Choice) {
                $9target(data.$10field);
                $11pick(uint256(1));
                data.field.$12inc();
                return Choice.$13A;
            }
        }
        "#,
        "/Navigation.sol",
    );

    fixture.check_queries(&[Query::Definition, Query::References(true)], 1..=14, str![[r#"
$1 definition: /Navigation.sol:0:8 library L {
$1 references: /Navigation.sol:0:8 library L {
/Navigation.sol:5:6 using L for uint256;
$2 definition: <none>
$2 references: /Navigation.sol:7:13 function f() external returns (uint256);
$3 definition: /Navigation.sol:11:20 function g(uint amount) public virtual {}
$3 references: /Navigation.sol:10:15 /// @param amount The amount.
/Navigation.sol:11:20 function g(uint amount) public virtual {}
$4 definition: /Navigation.sol:14:18 enum Choice { A, B }
$4 references: /Navigation.sol:14:18 enum Choice { A, B }
/Navigation.sol:27:22 return Choice.A;
$5 definition: /Navigation.sol:17:20 function g(uint value) public override {}
$5 references: /Navigation.sol:17:20 function g(uint value) public override {}
$6 definition: /Navigation.sol:16:19 uint256 public stateValue;
$6 references: /Navigation.sol:16:19 uint256 public stateValue;
/Navigation.sol:19:25 output = input + stateValue;
$7 definition: /Navigation.sol:22:13 function pick(string memory) public {}
$7 references: /Navigation.sol:22:13 function pick(string memory) public {}
$8 definition: /Navigation.sol:14:9 enum Choice { A, B }
$8 references: /Navigation.sol:14:9 enum Choice { A, B }
/Navigation.sol:23:54 function caller(Data memory data) public returns (Choice) {
/Navigation.sol:27:15 return Choice.A;
$9 definition: /Navigation.sol:18:13 function target(uint256 input) public view returns (uint256 output) {
$9 references: /Navigation.sol:18:13 function target(uint256 input) public view returns (uint256 output) {
/Navigation.sol:24:8 target(data.field);
$10 definition: /Navigation.sol:15:26 struct Data { uint256 field; }
$10 references: /Navigation.sol:15:26 struct Data { uint256 field; }
/Navigation.sol:24:20 target(data.field);
/Navigation.sol:26:13 data.field.inc();
$11 definition: /Navigation.sol:21:13 function pick(uint256) public {}
$11 references: /Navigation.sol:21:13 function pick(uint256) public {}
/Navigation.sol:25:8 pick(uint256(1));
$12 definition: /Navigation.sol:1:13 function inc(uint256 value) internal pure returns (uint256) {
$12 references: /Navigation.sol:1:13 function inc(uint256 value) internal pure returns (uint256) {
/Navigation.sol:26:19 data.field.inc();
$13 definition: /Navigation.sol:14:18 enum Choice { A, B }
$13 references: /Navigation.sol:14:18 enum Choice { A, B }
/Navigation.sol:27:22 return Choice.A;
$14 definition: /Navigation.sol:14:9 enum Choice { A, B }
$14 references: /Navigation.sol:14:9 enum Choice { A, B }
/Navigation.sol:23:54 function caller(Data memory data) public returns (Choice) {
/Navigation.sol:27:15 return Choice.A;

"#]]);
    fixture.check_queries(
        &[Query::Declaration],
        [2],
        str![[r#"
$2 /Navigation.sol:7:13 function f() external returns (uint256);

"#]],
    );
}
