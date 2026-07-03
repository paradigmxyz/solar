use super::support::RequestFixture;
use snapbox::str;

#[tokio::test(flavor = "current_thread")]
async fn completes_symbols_in_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol open
        contract C {
            uint256 stateValue;

            function target(uint256 input) public returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = $1localValue;
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture
        .check_completion(
            "$1",
            str![[r#"
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
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
output Variable
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property
target Method
tx Module

"#]],
        )
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn filters_locals_by_declaration_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            function f(uint256 input) public pure {
                uint256 localValue = $1input + 1;
                uint256 nextValue = $2localValue;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture
        .check_completion(
            "$1",
            str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$2",
            str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
        )
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn completes_dirty_members_from_typed_receivers() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract Token {
            uint256 public balance;
        }

        contract C {
            Token[] tokens;
            Token public token;
            Token foo;

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
        }
        "#,
        "/Completion.sol",
    );
    let expected = str![[r#"
balance Method

"#]];

    fixture.check_completion("$1", expected.clone()).await;
    fixture.check_completion("$2", expected.clone()).await;
    fixture.check_completion("$3", expected.clone()).await;
    fixture.check_completion("$4", expected.clone()).await;
    fixture.check_completion("$5", expected).await;
}

#[tokio::test(flavor = "current_thread")]
async fn completes_builtin_members_and_filters_globals() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            function f() public view {
                msg.$1;
                tx.$2;
                tx.$3
                block.$4;
                abi.$5;
                ms$6;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture
        .check_completion(
            "$1",
            str![[r#"
data Method
gas Method
sender Method
sig Method
value Method

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$2",
            str![[r#"
gasprice Method
origin Method

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$3",
            str![[r#"
gasprice Function
origin Function

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$4",
            str![[r#"
basefee Function
blobbasefee Function
chainid Function
coinbase Function
difficulty Function
gaslimit Function
number Function
prevrandao Function
timestamp Function

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$5",
            str![[r#"
decode Method
encode Method
encodeCall Method
encodePacked Method
encodeWithSelector Method
encodeWithSignature Method

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$6",
            str![[r#"
msg Module

"#]],
        )
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn completes_partial_member_prefixes_from_vfs_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            struct Data {
                uint256 field;
                uint256 other;
            }

            function f() public pure {
                Data memory data;
                data.$1;
                data.f$2;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture
        .check_completion(
            "$1",
            str![[r#"
field Property
other Property

"#]],
        )
        .await;
    fixture
        .check_completion(
            "$2",
            str![[r#"
field Property

"#]],
        )
        .await;
}
