use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn shows_declaration_signatures() {
    let fixture = RequestFixture::new(
        r#"
        //- /lib/forge-std/src/Script.sol
        abstract contract Script {}
        //- /src/Counter.sol
        contract Counter {}
        //- /script/Counter.s.sol open
        import {$1Script} from "../lib/forge-std/src/Script.sol";
        import {$2Counter} from "../src/Counter.sol";

        contract $3CounterScript is $4Script {
            Counter public counter;

            function run() public {
                counter = new $5Counter();
            }
        }

        contract Adder {
            function $6add(uint256 value) public pure {}

            function use() public pure {
                $7add(1);
            }
        }

        interface First {
            function value() external view returns (uint256);
        }
        interface Second {
            function value() external view returns (uint256);
        }
        contract Child is First, Second {
            uint256 public override(First, Second) $8value;
        }

        contract Balances {
            mapping(uint256 bucket => mapping(address account => uint256 amount)) private $9balances;

            function read(uint256 bucket, address account) external view returns (uint256) {
                return $10balances[bucket][account];
            }
        }

        type UserId is uint256;
        contract Variables {
            mapping(address => UserId) private $11ids;
            uint256 public constant $12LIMIT = 10;
            address immutable $13owner;
            uint256 public $14number;

            function use(UserId[] calldata $15values) external {
                UserId[] memory $16local;
                $17ids[msg.sender] = $18values[0];
                $19local = values;
                $20number++;
            }
        }

        contract Overloads {
            function pick(string memory value) public pure returns (string memory) { return value; }
            function pick(uint256 value) public pure returns (uint256) { return value; }

            function use() public pure returns (uint256) {
                return $21pick(1);
            }
        }

        contract Special {
            modifier $22limited(uint256 amount) { require(amount > 0); _; }
            $23constructor(uint256 count) payable { require(count > 0); }
            $24fallback() external payable {}
            $25receive() external payable {}
        }

        contract Base {
            constructor(uint256 initial) {}
        }
        contract Derived is Base {
            modifier guarded(uint256 threshold) { _; }
            $26constructor(uint256 initial) Base(initial + 1) {}
            function $27run(uint256 value) public guarded(value * 2) {}
        }
        "#,
        "/script/Counter.s.sol",
    );

    fixture.check_queries(
        &[Query::Hover],
        1..=27,
        str![[r#"
$1 0:8-0:14 abstract contract Script
$2 1:8-1:15 contract Counter
$3 2:9-2:22 contract CounterScript is Script
$4 2:26-2:32 abstract contract Script
$5 5:22-5:29 contract Counter
$6 9:13-9:16 function add(uint256 value) public pure
$7 11:8-11:11 function add(uint256 value) public pure
$8 21:43-21:48 uint256 public override(First, Second) value
$9 24:82-24:90 mapping(uint256 bucket => mapping(address account => uint256 amount)) private balances
$10 26:15-26:23 mapping(uint256 bucket => mapping(address account => uint256 amount)) private balances
$11 31:39-31:42 mapping(address => UserId) private ids
$12 32:28-32:33 uint256 public constant LIMIT
$13 33:22-33:27 address immutable owner
$14 34:19-34:25 uint256 public number
$15 35:35-35:41 UserId[] calldata values
$16 36:24-36:29 UserId[] memory local
$17 37:8-37:11 mapping(address => UserId) private ids
$18 37:26-37:32 UserId[] calldata values
$19 38:8-38:13 UserId[] memory local
$20 39:8-39:14 uint256 public number
$21 46:15-46:19 function pick(uint256 value) public pure returns (uint256)
$22 50:13-50:20 modifier limited(uint256 amount)
$23 51:4-51:15 constructor(uint256 count) payable
$24 52:4-52:12 fallback() external payable
$25 53:4-53:11 receive() external payable
$26 60:4-60:15 constructor(uint256 initial) Base(initial + 1)
$27 61:13-61:16 function run(uint256 value) public guarded(value * 2)

"#]],
    );
}

#[test]
fn includes_resolved_natspec_documentation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {
            /// @notice Updates the value.
            /// @param value The next value.
            /// @return result The stored value.
            function update(uint256 value) public pure virtual returns (uint256 result) {
                return value;
            }

            /// @notice Emitted after an update.
            /// @param value The emitted value.
            event Updated(uint256 indexed $6value) anonymous;

            /// @notice The account is forbidden.
            /// @param account The rejected account.
            error Forbidden(address account);

            /// @notice Chooses a value.
            /// @param first The first value.
            /// @param second The second value.
            /// @return firstOut The first result.
            /// @return secondOut The second result.
            function choose(uint256 first, uint256 second)
                public pure virtual returns (uint256 firstOut, uint256 secondOut)
            {
                return (first, second);
            }
        }
        //- /Use.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            modifier onlyReady() { _; }

            /// @inheritdoc Base
            function update(uint256 $4amount) public pure override onlyReady returns (uint256 $5out) {
                out = amount;
            }

            /// @inheritdoc Base
            function choose(uint256 second, uint256 third)
                public pure override returns (uint256 secondOut, uint256 thirdOut)
            {
                return (second, third);
            }

            function run(address account) public returns (uint256) {
                emit $1Updated(1);
                if (account == address(0)) {
                    revert $2Forbidden(account);
                }
                $7choose(1, 2);
                return $3update(1);
            }
        }

        contract Local {
            /// @notice Updates the stored value.
            /// @dev The caller is responsible for choosing the value.
            /// @param value The next value.
            /// @return result The normalized value.
            function set(uint256 $9value) public pure returns (uint256 $10result) {
                result = value;
            }

            /// @return The first return value.
            /// @return result The named return value.
            /// @return The final return value.
            function read() public pure returns (uint256, uint256 result, address) {
                return (1, 2, address(0));
            }

            function use() public pure {
                $8set(1);
                $11read();
            }
        }

        struct Record {
            uint256 value;
            address owner;
        }

        contract Getter {
            /// @return value The stored value.
            /// @return owner The record owner.
            Record public $12record;
        }

        interface RecordBase {
            /// @return first The first base value.
            /// @return second The second base value.
            function record() external view returns (uint256 first, address second);
        }

        contract InheritedGetter is RecordBase {
            /// @inheritdoc RecordBase
            Record public override $13record;
        }

        contract FirstChooser {
            /// @param right The first contract's left value.
            /// @param left The first contract's right value.
            function choose(uint256 right, uint256 left) public pure virtual {}
        }

        contract SecondChooser {
            /// @param left The second contract's left value.
            /// @param right The second contract's right value.
            function choose(uint256 left, uint256 right) public pure virtual {}
        }

        contract Chooser is FirstChooser, SecondChooser {
            /// @inheritdoc SecondChooser
            function choose(uint256 first, uint256 second)
                public pure override(FirstChooser, SecondChooser)
            {}

            function use() public pure {
                $14choose(1, 2);
            }
        }

        contract ReadBase {
            /// @param value The base value.
            /// @return result The base result.
            function read(uint256 value) public pure virtual returns (uint256 result) {}
        }

        contract ReadMiddle is ReadBase {
            /// @inheritdoc ReadBase
            function read(uint256 middleValue)
                public pure virtual override returns (uint256 middleResult)
            {}
        }

        contract ReadLeaf is ReadMiddle {
            /// @inheritdoc ReadMiddle
            function read(uint256 leafValue)
                public pure override returns (uint256 leafResult)
            {}

            function use() public pure {
                $15read(1);
            }
        }
        "#,
        "/Use.sol",
    );

    fixture.check_queries(&[Query::Hover], 1..=15, str![[r#"
$1 14:13-14:20 event Updated(uint256 indexed value) anonymous

Emitted after an update.

**@param**

- `value`: The emitted value.
$2 16:19-16:28 error Forbidden(address account)

The account is forbidden.

**@param**

- `account`: The rejected account.
$3 19:15-19:21 function update(uint256 amount) public pure override onlyReady returns (uint256 out)

Updates the value.

**@param**

- `amount`: The next value.

**@return**

- `out`: The stored value.
$4 4:28-4:34 uint256 amount

**@param**

- `amount`: The next value.
$5 4:84-4:87 uint256 out

**@return**

- `out`: The stored value.
$6 9:34-9:39 uint256 indexed value

**@param**

- `value`: The emitted value.
$7 18:8-18:14 function choose(uint256 second, uint256 third) public pure override returns (uint256 secondOut, uint256 thirdOut)

Chooses a value.

**@param**

- `second`: The first value.

- `third`: The second value.

**@return**

- `secondOut`: The first result.

- `thirdOut`: The second result.
$8 37:8-37:11 function set(uint256 value) public pure returns (uint256 result)

Updates the stored value.

**@dev**

The caller is responsible for choosing the value.

**@param**

- `value`: The next value.

**@return**

- `result`: The normalized value.
$9 27:25-27:30 uint256 value

**@param**

- `value`: The next value.
$10 27:61-27:67 uint256 result

**@return**

- `result`: The normalized value.
$11 38:8-38:12 function read() public pure returns (uint256, uint256 result, address)

**@return**

- The first return value.

- `result`: The named return value.

- The final return value.
$12 48:18-48:24 Record public record

**@return**

- `value`: The stored value.

- `owner`: The record owner.
$13 57:27-57:33 Record public override record

**@return**

- `value`: The first base value.

- `owner`: The second base value.
$14 75:8-75:14 function choose(uint256 first, uint256 second) public pure override(FirstChooser, SecondChooser)

**@param**

- `first`: The second contract's left value.

- `second`: The second contract's right value.
$15 95:8-95:12 function read(uint256 leafValue) public pure override returns (uint256 leafResult)

**@param**

- `leafValue`: The base value.

**@return**

- `leafResult`: The base result.

"#]]);
}

#[test]
fn skips_invalid_documentation_and_non_symbol_positions() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Unsupported.sol open
        contract $1C {
            /// A user ID.
            type $6UserId is uint256;
            /// Stored data.
            struct $2Data {}
            /// An available kind.
            enum $3Kind { A }

            function use() public returns (uint256) {
                $7uint256 value = $4missing;
                return $8 1;
            }

            function empty() public { $5
            }

            function pick(uint8 value) internal pure returns (uint8) {
                return value;
            }

            function pick(uint256 value) internal pure returns (uint256) {
                return value;
            }

            function call(uint8 value) public pure returns (uint256) {
                return $9pick(value);
            }

            /// @return wrong This tag is invalid.
            /// @return The unnamed return value.
            function read() public pure returns (uint256 first, uint256) {
                return (1, 2);
            }

            function useRead() public pure {
                $10read();
            }
        }

        contract Base {
            /// @param value The inherited value.
            /// @return result The inherited result.
            function update(uint256 value) public pure virtual returns (uint256 result) {}
        }

        contract Child is Base {
            /// @param missing This tag is invalid.
            /// @return missing This tag is also invalid.
            /// @inheritdoc Base
            function update(uint256 renamed)
                public pure override returns (uint256 renamedResult)
            {}

            function use() public pure {
                $11update(1);
            }
        }
        "#,
        "/Unsupported.sol",
    );

    fixture.check_queries(
        &[Query::Hover],
        1..=11,
        str![[r#"
$1 0:9-0:10 contract C
$2 4:11-4:15 struct Data

Stored data.
$3 6:9-6:13 enum Kind

An available kind.
$4 <none>
$5 <none>
$6 2:9-2:15 type UserId is uint256

A user ID.
$7 <none>
$8 <none>
$9 <none>
$10 28:8-28:12 function read() public pure returns (uint256 first, uint256)

**@return**

- The unnamed return value.
$11 44:8-44:14 function update(uint256 renamed) public pure override returns (uint256 renamedResult)

**@param**

- `renamed`: The inherited value.

**@return**

- `renamedResult`: The inherited result.

"#]],
    );
}

#[test]
fn shows_builtin_signature() {
    let fixture = RequestFixture::new(
        r#"
        //- /Builtins.sol open
        contract C {
            function hash(bytes memory data) external pure returns (bytes32) {
                return $1keccak256(data);
            }
        }
        "#,
        "/Builtins.sol",
    );

    fixture.check_queries(
        &[Query::Hover],
        [1],
        str![[r#"
$1 2:15-2:24 function keccak256(bytes memory) pure returns (bytes32)

"#]],
    );
    fixture.check_queries(
        &[Query::Definition, Query::Declaration],
        [1],
        str![[r#"
$1 definition: <none>
$1 declaration: <none>

"#]],
    );
}

#[test]
fn builtin_hover_distinguishes_selectors_and_bound_array_overloads() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol open
        contract C {
            event E(uint256 value);
            error Bad(uint256 value);
            uint256[] values;
            function f() external {}
            function inspect() external view returns (bytes4, bytes32, bytes4, address) {
                return (this.f.$1selector, E.$2selector, Bad.$3selector, /* 😀 */ msg.$4sender);
            }
            function mutate() external {
                values.$5push();
                values.$6push(1);
                values.$7pop();
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_queries(
        &[Query::Hover],
        1..=7,
        str![[r#"
$1 6:23-6:31 bytes4 function.selector
$2 6:35-6:43 bytes32 event.selector
$3 6:49-6:57 bytes4 error.selector
$4 6:72-6:78 address msg.sender
$5 9:15-9:19 function array.push() returns (uint256)
$6 10:15-10:19 function array.push(uint256)
$7 11:15-11:18 function array.pop()

"#]],
    );
    fixture.check_queries(
        &[Query::Definition, Query::Declaration],
        1..=7,
        str![[r#"
$1 definition: <none>
$1 declaration: <none>
$2 definition: <none>
$2 declaration: <none>
$3 definition: <none>
$3 declaration: <none>
$4 definition: <none>
$4 declaration: <none>
$5 definition: <none>
$5 declaration: <none>
$6 definition: <none>
$6 declaration: <none>
$7 definition: <none>
$7 declaration: <none>

"#]],
    );
}
