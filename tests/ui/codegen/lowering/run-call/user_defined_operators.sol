//@ run-call: addressEq [0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000002] => true
//@ run-call: addressEq [0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003] => false
//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: add_ 3, 4 => 7
//@ run-call: mul_ 3, 4 => 12
//@ run-call: eq_ 3, 4 => false
//@ run-call: eq_ 4, 4 => true

type Balance is uint256;
type Account is address;

function sameAccount(Account a, Account b) pure returns (bool) {
    return Account.unwrap(a) == Account.unwrap(b);
}

using {sameAccount as ==} for Account global;

function add(Balance a, Balance b) pure returns (Balance) {
    return Balance.wrap(Balance.unwrap(a) + Balance.unwrap(b));
}
function mul(Balance a, Balance b) pure returns (Balance) {
    return Balance.wrap(Balance.unwrap(a) * Balance.unwrap(b));
}
function eq(Balance a, Balance b) pure returns (bool) {
    return Balance.unwrap(a) == Balance.unwrap(b);
}

using {add as +, mul as *, eq as ==} for Balance global;

contract UserDefinedOperators {
    function addressEq(Account[] memory accounts) external pure returns (bool) {
        return accounts[0] == accounts[1];
    }

    function add_(Balance a, Balance b) external pure returns (uint) {
        return Balance.unwrap(a + b);
    }
    function mul_(Balance a, Balance b) external pure returns (uint) {
        return Balance.unwrap(a * b);
    }
    function eq_(Balance a, Balance b) external pure returns (bool) {
        return a == b;
    }
}
