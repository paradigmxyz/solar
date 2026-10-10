use super::support::{RequestFixture, reanalyze};
use crate::{
    symbols::CompletionContext,
    test_support::{assert_polls, request_params},
};
use serde_json::json;
use snapbox::{IntoData, str};

#[test]
fn completes_line_natspec_templates() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        // 😀
        ///$1
        contract Vault {}
        ///$2
        abstract contract AbstractVault {}
        ///$3
        interface IVault {}
        ///$4
        library VaultMath {}
        contract C {
            struct Record {
                uint256 amount;
                address owner;
                uint256[] samples;
                mapping(address account => uint256 balance) balances;
            }
            ///$5
            function value(uint256 amount, uint256) external pure returns (uint256 total) {
                return amount;
            }
            ///$6
            function other(uint256, address recipient) external pure returns (uint256) {
                return uint160(recipient);
            }
            ///$7
            function dollars(uint256 $amount) external pure returns (uint256 $result) {
                $result = $amount;
            }
            ///$8
            constructor(uint256 ownerSeed, address) {}
            ///$9
            fallback(bytes calldata input) external returns (bytes memory output) {
                output = input;
            }
            ///$10
            receive() external payable {}
            ///$11
            event Transfer(address indexed from, address indexed, uint256 amount);
            ///$12
            error TransferFailed(uint256 code, address);
            ///$13
            struct Pair {
                uint256 amount;
                address owner;
            }
            ///$14
            enum Status { Pending, Complete }
            ///$15
            uint256 public total;
            ///$16
            Record public record;
            ///$17
            uint256 private secret;
            ///$18
            uint256 internal cached;
            ///$19
            modifier onlyOwner() { _; }
            //$20
            function first() external {}
            //*$21
            function second() external {}
            /*$22 */
            function third() external {}
        }
        ////$23
        contract FourSlashes {}
        /**/$24
        contract EmptyBlock {}
        /***/$25
        contract ThreeStars {}
        /// existing documentation$26
        contract NonEmpty {}
        ///$27
        // intervening comment
        contract Separated {}
        ///$28
        type Price is uint256;
        "#,
        "/Completion.sol",
    );

    fixture.check_completions(
        &[
            "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11", "$12", "$13",
            "$14", "$15", "$16", "$17", "$18", "$19", "$23", "$24", "$25", "$26", "$27", "$28",
        ],
        str![[r#"
$1:
NatSpec contract documentation Snippet detail="contract Vault" sort="0" filter="///" format=Snippet edit=1:0-1:3
| /// @title $1
| /// @author $2
| /// @notice $3$0
$2:
NatSpec abstract contract documentation Snippet detail="abstract contract AbstractVault" sort="0" filter="///" format=Snippet edit=3:0-3:3
| /// @title $1
| /// @author $2
| /// @notice $3$0
$3:
NatSpec interface documentation Snippet detail="interface IVault" sort="0" filter="///" format=Snippet edit=5:0-5:3
| /// @title $1
| /// @author $2
| /// @notice $3$0
$4:
NatSpec library documentation Snippet detail="library VaultMath" sort="0" filter="///" format=Snippet edit=7:0-7:3
| /// @title $1
| /// @author $2
| /// @notice $3$0
$5:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=16:4-16:7
| /// $1
|     /// @param amount $2
|     /// @return total $3$0
$6:
NatSpec function documentation Snippet detail="function other" sort="0" filter="///" format=Snippet edit=20:4-20:7
| /// $1
|     /// @param recipient $2
|     /// @return $3$0
$7:
NatSpec function documentation Snippet detail="function dollars" sort="0" filter="///" format=Snippet edit=24:4-24:7
| /// $1
|     /// @param \$amount $2
|     /// @return \$result $3$0
$8:
NatSpec constructor documentation Snippet detail="constructor" sort="0" filter="///" format=Snippet edit=28:4-28:7
| /// $1
|     /// @param ownerSeed $2$0
$9:
NatSpec fallback documentation Snippet detail="fallback" sort="0" filter="///" format=Snippet edit=30:4-30:7
| /// $1
|     /// @param input $2
|     /// @return output $3$0
$10:
NatSpec receive documentation Snippet detail="receive" sort="0" filter="///" format=Snippet edit=34:4-34:7
| /// $1$0
$11:
NatSpec event documentation Snippet detail="event Transfer" sort="0" filter="///" format=Snippet edit=36:4-36:7
| /// $1
|     /// @param from $2
|     /// @param amount $3$0
$12:
NatSpec error documentation Snippet detail="error TransferFailed" sort="0" filter="///" format=Snippet edit=38:4-38:7
| /// $1
|     /// @param code $2$0
$13:
NatSpec struct documentation Snippet detail="struct Pair" sort="0" filter="///" format=Snippet edit=40:4-40:7
| /// $1
|     /// @param amount $2
|     /// @param owner $3$0
$14:
NatSpec enum documentation Snippet detail="enum Status" sort="0" filter="///" format=Snippet edit=45:4-45:7
| /// $1$0
$15:
NatSpec public state variable documentation Snippet detail="public state variable total" sort="0" filter="///" format=Snippet edit=47:4-47:7
| /// @notice $1
|     /// @return $2$0
$16:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=49:4-49:7
| /// @notice $1
|     /// @return amount $2
|     /// @return owner $3$0
$17:
NatSpec private state variable documentation Snippet detail="private state variable secret" sort="0" filter="///" format=Snippet edit=51:4-51:7
| /// @dev $1$0
$18:
NatSpec internal state variable documentation Snippet detail="internal state variable cached" sort="0" filter="///" format=Snippet edit=53:4-53:7
| /// @dev $1$0
$19 $23 $24 $25 $26 $27 $28:

"#]],
    );
    fixture.check_completions_in(&mut fixture.state(), &["$1", "$7"], str![[r#"
$1:
NatSpec contract documentation Snippet detail="contract Vault" sort="0" filter="///" format=PlainText edit=1:0-1:3
| /// @title
| /// @author
| /// @notice
$7:
NatSpec function documentation Snippet detail="function dollars" sort="0" filter="///" format=PlainText edit=24:4-24:7
| ///
|     /// @param $amount
|     /// @return $result

"#]]);
    fixture.check_triggered_completions(&[("$20", "/"), ("$21", "*"), ("$22", "*")], str![""]);
}

#[test]
fn completes_block_and_recovered_natspec() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract Duplicate {
            ///$1
            function value(uint256 amount, uint256 amount)
                external
                pure
                returns (uint256 total, uint256)
            {
                return (amount, amount);
            }
        }
        interface Base { function value(uint256 amount) external; }
        contract Child is Base {
            ///$2
            function value(address account) external override {}
        }
        /**$3 */
        contract Vault {}
        /**$4 */ contract SameLine {}
        /**$5
         *
         */
        contract Multiline {}
        /** docs */ contract Closed { function f() external pure { ret$8urn; } }
        ///$6
        contract LineDocs {}
        /**$7
        contract OpenVault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completions(&["$1", "$2", "$3", "$4", "$5", "$8"], str![[r#"
$1:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=1:4-1:7
| /// $1
|     /// @param amount $2
|     /// @return total $3
|     /// @return $4$0
$2:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=12:4-12:7
| /// $1
|     /// @param account $2$0
$3:
NatSpec contract documentation Snippet detail="contract Vault" sort="0" filter="/**" format=Snippet edit=15:0-15:6
| /**
|  * @title $1
|  * @author $2
|  * @notice $3$0
|  */
$4:
NatSpec contract documentation Snippet detail="contract SameLine" sort="0" filter="/**" format=Snippet edit=17:0-17:6
| /**
|  * @title $1
|  * @author $2
|  * @notice $3$0
|  */
$5:
NatSpec contract documentation Snippet detail="contract Multiline" sort="0" filter="/**" format=Snippet additional=18:3-20:3="" edit=18:0-18:3
| /**
|  * @title $1
|  * @author $2
|  * @notice $3$0
|  */
$8:
revert Function

"#]]);
    fixture.check_triggered_completions(&[("$6", "/"), ("$7", "*")], str![[r#"
$6:
NatSpec contract documentation Snippet detail="contract LineDocs" sort="0" filter="///" format=Snippet edit=23:0-23:3
| /// @title $1
| /// @author $2
| /// @notice $3$0
$7:
NatSpec contract documentation Snippet detail="contract OpenVault" sort="0" filter="/**" format=Snippet edit=25:0-25:3
| /**
|  * @title $1
|  * @author $2
|  * @notice $3$0
|  */

"#]]);

    // A clean file also falls back to ordinary completion after a closed block comment.
    let clean = r#"
        //- /Completion.sol open
        /** docs */ contract C { function f() external pure { ret$1urn; } }
        "#;
    RequestFixture::new(clean, "/Completion.sol").check_completions(
        &["$1"],
        str![[r#"
revert Function

"#]],
    );
}

#[test]
fn completes_inheritdoc_templates() {
    let source = r#"
        //- /Base.sol
        interface Original {
            function value() external view returns (uint256 result);
        }
        interface Hidden { function hidden() external; }

        //- /Middle.sol
        import {Hidden as Reexported} from "./Base.sol";

        //- /Completion.sol open
        import {Original as Alias} from "./Base.sol";
        import "./Middle.sol";
        interface First { function value(uint256 amount) external view returns (uint256 total); }
        interface Second { function value(uint256 amount) external view returns (uint256 total); }
        interface $Base { function value() external; }
        contract Child is First, Second {
            ///$1
            function value(uint256 amount)
                external
                pure
                override(First, Second)
                returns (uint256 total)
            {
                total = amount;
            }
        }
        contract FallbackBase { fallback() external virtual {} }
        contract FallbackChild is FallbackBase {
            ///$2
            fallback() external override {}
        }
        contract AliasChild is Alias {
            ///$3
            function value() external pure override returns (uint256 result) {
                result = 1;
            }
        }
        contract ReexportedChild is Reexported {
            ///$4
            function hidden() external override {}
        }
        contract DollarChild is $Base {
            ///$5
            function value() external override {}
        }
        "#;
    let fixture = RequestFixture::new(source, "/Completion.sol");

    fixture.check_completions(&["$1", "$2", "$3", "$4", "$5"], str![[r#"
$1:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=6:4-6:7
| /// $1
|     /// @param amount $2
|     /// @return total $3$0
NatSpec @inheritdoc First Snippet detail="Inherit documentation from First" sort="1:First" filter="///" format=Snippet edit=6:4-6:7
| /// @inheritdoc First$0
NatSpec @inheritdoc Second Snippet detail="Inherit documentation from Second" sort="1:Second" filter="///" format=Snippet edit=6:4-6:7
| /// @inheritdoc Second$0
$2:
NatSpec fallback documentation Snippet detail="fallback" sort="0" filter="///" format=Snippet edit=18:4-18:7
| /// $1$0
NatSpec @inheritdoc FallbackBase Snippet detail="Inherit documentation from FallbackBase" sort="1:FallbackBase" filter="///" format=Snippet edit=18:4-18:7
| /// @inheritdoc FallbackBase$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=22:4-22:7
| /// $1
|     /// @return result $2$0
NatSpec @inheritdoc Alias Snippet detail="Inherit documentation from Alias" sort="1:Alias" filter="///" format=Snippet edit=22:4-22:7
| /// @inheritdoc Alias$0
$4:
NatSpec function documentation Snippet detail="function hidden" sort="0" filter="///" format=Snippet edit=28:4-28:7
| /// $1$0
NatSpec @inheritdoc Reexported Snippet detail="Inherit documentation from Reexported" sort="1:Reexported" filter="///" format=Snippet edit=28:4-28:7
| /// @inheritdoc Reexported$0
$5:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=32:4-32:7
| /// $1$0
NatSpec @inheritdoc $Base Snippet detail="Inherit documentation from $Base" sort="1:$Base" filter="///" format=Snippet edit=32:4-32:7
| /// @inheritdoc \$Base$0

"#]]);

    // Accumulated batch results index the aliased base too.
    let fixture = RequestFixture::new_in_batches(source, &["/Base.sol", "/Completion.sol"]);
    fixture.check_completions(&["$3"], str![[r#"
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=22:4-22:7
| /// $1
|     /// @return result $2$0
NatSpec @inheritdoc Alias Snippet detail="Inherit documentation from Alias" sort="1:Alias" filter="///" format=Snippet edit=22:4-22:7
| /// @inheritdoc Alias$0

"#]]);
}

#[test]
fn pending_analysis_reuses_only_current_natspec_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        struct Record {
            uint256 amount;
            address owner;
        }
        interface Base { function value() external; }
        interface Other { function value() external; }

        //- /Completion.sol open
        import {Record, Base, Other} from "./Base.sol";
        contract C {
            // $1
            Record public record;
            // $2
            uint256 public total;
        }
        contract Child is Base {
            // $3
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let completion = fixture.project_contents("/Completion.sol");
    let docs = completion.replace("// ", "///");
    let (before, after) = completion.rsplit_once("// ").unwrap();
    let markers = ["$1", "$2", "$3"];

    // Trivia-only edits keep the analyzed semantics.
    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &docs)]);
    fixture.check_completions_in(&mut state, &markers, str![[r#"
$1:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=2:4-2:7
| /// @notice $1
|     /// @return amount $2
|     /// @return owner $3$0
$2:
NatSpec public state variable documentation Snippet detail="public state variable total" sort="0" filter="///" format=Snippet edit=4:4-4:7
| /// @notice $1
|     /// @return $2$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=8:4-8:7
| /// $1$0
NatSpec @inheritdoc Base Snippet detail="Inherit documentation from Base" sort="1:Base" filter="///" format=Snippet edit=8:4-8:7
| /// @inheritdoc Base$0

"#]]);
    let first_block = completion.replacen("// ", "/**", 1);
    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &first_block)]);
    fixture.check_completions_in(&mut state, &["$1"], str![[r#"
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="/**" format=Snippet edit=2:4-2:7
| /**
|      * @notice $1
|      * @return amount $2
|      * @return owner $3$0
|      */

"#]]);
    let last_block = format!("{before}/**{after}");
    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &last_block)]);
    fixture.check_completions_in(&mut state, &["$3"], str![[r#"
NatSpec function documentation Snippet detail="function value" sort="0" filter="/**" format=Snippet edit=8:4-8:7
| /**
|      * $1$0
|      */
NatSpec @inheritdoc Base Snippet detail="Inherit documentation from Base" sort="1:Base" filter="/**" format=Snippet edit=8:4-8:7
| /**
|      * @inheritdoc Base$0
|      */

"#]]);

    // Syntax changes use the current VFS and omit stale semantics.
    let changed = docs.replace("public total", "private total").replace("is Base", "is Other");
    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &changed)]);
    fixture.check_completions_in(&mut state, &markers, str![[r#"
$1:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=2:4-2:7
| /// @notice $1$0
$2:
NatSpec private state variable documentation Snippet detail="private state variable total" sort="0" filter="///" format=Snippet edit=4:4-4:7
| /// @dev $1$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=8:4-8:7
| /// $1$0

"#]]);
    let base = fixture.project_contents("/Base.sol").replace("owner", "admin");
    let mut state =
        fixture.completion_state_after_changes(&[("/Base.sol", &base), ("/Completion.sol", &docs)]);
    fixture.check_completions_in(&mut state, &["$1", "$3"], str![[r#"
$1:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=2:4-2:7
| /// @notice $1$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=8:4-8:7
| /// $1$0

"#]]);

    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &docs)]);
    state.mark_context_analysis_pending_for_test();
    fixture.check_completions_in(&mut state, &["$1", "$3"], str![[r#"
$1:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=2:4-2:7
| /// @notice $1$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=8:4-8:7
| /// $1$0

"#]]);

    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &docs)]);
    let base = fixture.project_path("/Base.sol");
    state.mark_source_analysis_pending_for_test(base.clone());
    std::fs::remove_file(base).unwrap();
    fixture.check_completions_in(&mut state, &["$1", "$3"], str![[r#"
$1:
NatSpec public state variable documentation Snippet detail="public state variable record" sort="0" filter="///" format=Snippet edit=2:4-2:7
| /// @notice $1$0
$3:
NatSpec function documentation Snippet detail="function value" sort="0" filter="///" format=Snippet edit=8:4-8:7
| /// $1$0

"#]]);
}

#[test]
fn completes_symbols_in_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol open
        contract C {
            uint256 stateValue;
            uint256 other = $1stateValue;

            function target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = $2input + stateValue;
                output = $3localValue;
            }
        }
        // trailing comment
        "#,
        "/Symbols.sol",
    );

    fixture.check_completions(
        &["$1", "$2", "$3"],
        str![[r#"
$1:
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
gasleft Function
keccak256 Function
msg Module
mulmod Function
other Property detail="C"
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property detail="C"
target Method detail="C"
tx Module
$2:
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
gasleft Function
input Variable detail="target"
keccak256 Function
msg Module
mulmod Function
other Property detail="C"
output Variable detail="target"
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property detail="C"
target Method detail="C"
tx Module
$3:
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
gasleft Function
input Variable detail="target"
keccak256 Function
localValue Variable detail="target"
msg Module
mulmod Function
other Property detail="C"
output Variable detail="target"
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property detail="C"
target Method detail="C"
tx Module

"#]],
    );
}

#[test]
fn does_not_complete_inside_comments_or_strings() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            function f() public pure {
                // sentence $1
                /* sentence
                 * $2 */
                string memory value = "sentence // $3";
                value$4;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completions(
        &["$1", "$2", "$3", "$4"],
        str![[r#"
$1 $2 $3:
$4:
value Variable detail="f"

"#]],
    );
}

#[test]
fn completes_members_and_filters_prefixes() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract Token {
            uint256 public balance;
        }

        library Math {
            function twice(uint256 value) internal pure returns (uint256) { return value * 2; }
            function wrong(address value) internal pure returns (address) { return value; }
            function hidden(uint256 value) private pure returns (uint256) { return value; }
        }
        function triple(uint256 value) pure returns (uint256) { return value * 3; }
        contract Library {
            using Ma$16th for uint256;
            using {triple} for uint256;
            function f(uint256 value) public pure {
                Math.$17;
                Math.tw$18;
                value.$19;
                value.tw$20;
                (value + 1).$21;
            }
        }
        contract NoUsing {
            function f(uint256 value) public pure {
                value.$22;
                (value + 1).$23;
                missing.$24;
                unknown().$25;
                value . $26;
            }
        }

        contract C {
            struct Data {
                uint256 field;
                uint256 other;
            }

            Token[] tokens;
            Token public token;
            Token foo;
            uint256 needleValue;

            function getToken() public view returns (Token) {
                return token;
            }

            function read(uint256 i) public view {
                getToken().$1;
                (this.token()).$2b;
                tokens[i].bal$3;
                foo.$4;
                foo
                    .bal$5;
            }

            function f() public view {
                Data memory data;
                uint256 needleValue = 1;
                msg.$6;
                tx.$7;
                tx.$8
                block.$9;
                abi.$10;
                ms$11;
                data.$12;
                data.f$13;
                nDV$14;
                noMatchingName$15;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completions(
        &[
            "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11", "$12", "$13",
            "$14", "$15", "$16", "$17", "$18", "$19", "$20", "$21", "$22", "$23", "$24", "$25",
            "$26",
        ],
        str![[r#"
$1 $2 $3 $4 $5:
balance Method
$6:
data Method
gas Method
sender Method
sig Method
value Method
$7:
gasprice Method
origin Method
$8:
gasprice Function
origin Function
$9:
basefee Function
blobbasefee Function
chainid Function
coinbase Function
difficulty Function
gaslimit Function
number Function
prevrandao Function
slotnum Function
timestamp Function
$10:
decode Method
encode Method
encodeCall Method
encodePacked Method
encodeWithSelector Method
encodeWithSignature Method
$11:
msg Module
$12:
field Property detail="Data"
other Property detail="Data"
$13:
field Property detail="Data"
$14:
needleValue Variable detail="f"
$15 $22 $23 $24 $25 $26:
$16:
Math Module
$17:
twice Method detail="Math"
wrong Method detail="Math"
$18 $20:
twice Method detail="Math"
$19 $21:
triple Function
twice Method detail="Math"

"#]],
    );
}

#[test]
fn member_completion_cache_preserves_source_and_contract_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Libraries.sol
        library First {
            function first(uint256 value) internal pure returns (uint256) { return value; }
        }
        library Second {
            function second(uint256 value) internal pure returns (uint256) { return value; }
        }
        //- /Other.sol open
        import {First} from "./Libraries.sol";
        using First for uint256;
        function readOther(uint256 value) pure { value.$1; }
        //- /Completion.sol open
        import {Second} from "./Libraries.sol";
        import "./Other.sol";
        function readGlobal(uint256 value) pure { value.$5; }
        contract C {
            using Second for uint256;
            function read(uint256 value) public pure {
                value.$2;
                value.$3;
            }
        }
        contract D {
            function read(uint256 value) public pure { value.$4; }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completions(
        &["$1", "$2", "$3", "$4", "$5"],
        str![[r#"
$1:
first Method detail="First"
$2 $3:
second Method detail="Second"
$4 $5:

"#]],
    );
}

#[test]
fn completes_members_with_incomplete_syntax() {
    for (prelude, expression) in [
        ("using Math for uint256;", "x.$1"),
        ("using Math for uint256;", "x.tw$1"),
        ("using Math for uint256;", "(x + 1).$1"),
        ("using Math for uint256;", "Math.$1"),
        ("using Math for uint256;", "uint broken = ;\nx.$1"),
        ("using Math for uint256;", "missing();\nx.$1"),
        ("", "x.$1"),
    ] {
        for ending in ["\n}\n}", ""] {
            let fixture = RequestFixture::new_allowing_diagnostics(
                &format!(
                    r#"
                    //- /Completion.sol open
                    library Math {{
                        function twice(uint256 value) internal pure returns (uint256) {{
                            return value * 2;
                        }}
                    }}
                    contract C {{
                        {prelude}
                        function f() public pure {{
                            uint x;
                            {expression}{ending}
                    "#,
                ),
                "/Completion.sol",
            );
            let expected = if prelude.is_empty() { "" } else { "twice Method detail=\"Math\"\n" };
            fixture.check_completions(&["$1"], expected);
        }
    }
}

#[test]
fn completes_members_before_a_following_statement() {
    for expression in [
        "tokens[i].$1\n                next();",
        "tokens[i].$1\n                tokens[i].balance;",
        "tokens[i].\n                $1balance;",
        "tokens[i] .$1\n                next();",
        "tokens[i] /* . 😀 */ .$1\n                next();",
        "getToken().$1\n                next();",
    ] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Completion.sol open
                contract Token {{
                    uint256 public balance;
                }}
                contract C {{
                    Token[] tokens;
                    function getToken() internal view returns (Token) {{ return tokens[0]; }}
                    function f(uint256 i) public view {{
                        {expression}
                    }}
                }}
                "#,
            ),
            "/Completion.sol",
        );
        fixture.check_completions(
            &["$1"],
            str![[r#"
balance Method

"#]],
        );
    }
}

#[test]
fn keeps_lexical_completion_before_member_dot() {
    for expression in ["tokens[i] $1 .balance;", "tokens[i] /* . 😀 */ $1 .balance;"] {
        let fixture = RequestFixture::new(
            &format!(
                r#"
                //- /Completion.sol open
                contract Token {{
                    uint256 public balance;
                }}
                contract C {{
                    Token[] tokens;
                    function f(uint256 i) public view {{
                        {expression}
                    }}
                }}
                "#,
            ),
            "/Completion.sol",
        );
        let state = fixture.completion_state();
        let (uri, position) = fixture.marker_location("$1");
        let items = state.symbol_tables.load().completion_items(
            &uri,
            position,
            CompletionContext::new("tokens", None),
        );
        assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["tokens"]);
    }
}

#[test]
fn getter_member_completion_does_not_extend_past_declaration() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record { uint256 value; }
            Record[] public records; $1
            uint256 lexicalNeedle;
        }
        "#,
        "/Completion.sol",
    );
    let state = fixture.completion_state();
    let (uri, position) = fixture.marker_location("$1");
    let items = state.symbol_tables.load().completion_items(
        &uri,
        position,
        CompletionContext::new("lexicalNeedle", None),
    );
    assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["lexicalNeedle"]);
}

#[test]
fn completes_all_declaration_receivers_after_edits() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        enum Status { Pending, Done }
        struct Record { uint value; }
        type Price is uint256;
        event Changed(uint value);
        error Failed(uint value);
        function helper(uint value) pure returns (uint) { return value; }
        contract Base {
            uint public total;
            function inherited() internal pure {}
            function hidden() private pure {}
            function externalCall() public pure {}
        }
        contract C is Base {
            using {helper} for uint256;
            function f() public pure {
                Status;$1
                Record;$2
                Price;$3
                Changed;$4
                Failed;$5
                helper;$6
                Base;$7
            }
            function g(function() external callback) public pure {
                callback;$9
            }
        }
        contract Other {
            function f() public pure { Base;$8 }
        }
        "#,
        "/Completion.sol",
    );
    check_member_access_edits(
        &fixture,
        &["Status", "Record", "Price", "Changed", "Failed", "helper", "Base", "callback"],
        &["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9"],
        str![[r#"
$1:
Done EnumMember detail="Status"
Pending EnumMember detail="Status"
$2 $6:
$3:
unwrap Method
wrap Method
$4 $5:
selector Method
$7:
externalCall Method detail="Base"
inherited Method detail="Base"
total Property detail="Base"
$8:
externalCall Method detail="Base"
total Method
$9:
address Method
selector Method

"#]],
    );
}

#[test]
fn completes_namespace_and_library_receivers_after_edits() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Definitions.sol
        enum Status { Pending, Done }
        library Math {
            function twice(uint x) internal pure returns (uint) { return x * 2; }
            function hidden(uint x) private pure returns (uint) { return x; }
        }
        //- /Exports.sol
        import {Math as Numbers} from "./Definitions.sol";
        //- /Completion.sol open
        import * as Definitions from "./Definitions.sol";
        import "./Exports.sol" as Exports;
        import {Math as Numbers} from "./Definitions.sol";
        contract C {
            function f() public pure {
                Def$1initions;$2
                Exports;$3
            }
        }
        contract D {
            using Nu$4mbers for uint256;
            function f(uint256 value) public pure {
                uint x = 1;
                Numbers;$5
                value;$6
                x;$7
            }
        }
        "#,
        &["/Definitions.sol", "/Completion.sol"],
    );
    fixture.check_completions(
        &["$1", "$4"],
        str![[r#"
$1:
Definitions Module
$4:
Numbers Module

"#]],
    );
    check_member_access_edits(
        &fixture,
        &["Definitions", "Exports", "Numbers", "value", "x"],
        &["$2", "$3", "$5", "$6", "$7"],
        str![[r#"
$2:
Math Module
Status Enum
$3:
Numbers Module
$5 $6 $7:
twice Method detail="Math"

"#]],
    );
}

#[test]
fn edited_receivers_use_the_callers_scope() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Base.sol
        contract Base {
            uint internal amount;
            uint private secret;
            function externalCall() public pure {}
            function inherited() internal pure {}
        }
        //- /Completion.sol open
        import {Base} from "./Base.sol";
        function twice(uint x) pure returns (uint) { return x * 2; }
        contract C is Base {
            using {twice} for uint256;
            function f() public pure {
                amount;$1
                this;$2
                super;$3
                externalCall;$4
                secret;$5
            }
        }
        contract Other is Base {
            function f() public pure { amount;$6 }
        }
        contract Shadowing {
            struct Data { uint field; }
            function f() public pure {
                Data memory msg;
                Data memory field;
                msg;$7
                msg.field;$8
            }
        }
        "#,
        "/Completion.sol",
    );
    check_member_access_edits(
        &fixture,
        &["amount", "this", "super", "externalCall", "secret", "msg", "msg.field"],
        &["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8"],
        str![[r#"
$1:
twice Function
$2:
externalCall Method detail="Base"
f Method detail="C"
$3:
externalCall Method detail="Base"
inherited Method detail="Base"
$4:
selector Method
$5 $6 $8:
$7:
field Property detail="Data"

"#]],
    );
}

/// Checks completions after turning each `name;` in `/Completion.sol` into the unanalyzed
/// member access `name.`.
fn check_member_access_edits(
    fixture: &RequestFixture,
    names: &[&str],
    markers: &[&str],
    expected: impl IntoData,
) {
    let changed =
        names.iter().fold(fixture.project_contents("/Completion.sol"), |contents, name| {
            // Keep each edited receiver a separate statement so reanalysis cannot join it with
            // the next line, and leave declarations such as `Data memory msg;` intact.
            let statement = format!("{name};");
            let mut changed = String::with_capacity(contents.len());
            let mut rest = contents.as_str();
            while let Some(start) = rest.find(&statement) {
                let (before, after) = rest.split_at(start);
                changed.push_str(before);
                let leading = changed.trim_end().ends_with(['\n', '{', ';']);
                changed.push_str(name);
                changed.push_str(if leading { ".;" } else { ";" });
                rest = &after[statement.len()..];
            }
            changed.push_str(rest);
            changed
        });
    let mut state = fixture.completion_state_after_changes(&[("/Completion.sol", &changed)]);
    // Member completion waits for the edited source's analysis instead of reusing the old one.
    for marker in markers {
        let (uri, position) = fixture.marker_location(marker);
        let params = request_params(&uri, position, json!({}));
        assert_polls(true, crate::handlers::completion(&mut state, params));
    }
    reanalyze(&state);
    fixture.check_completions_in(&mut state, markers, expected);
}
