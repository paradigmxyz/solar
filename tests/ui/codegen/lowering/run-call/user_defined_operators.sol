//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: add_(uint256,uint256) 3, 4 => 7
//@ run-call: mul_(uint256,uint256) 3, 4 => 12
//@ run-call: eq_(uint256,uint256) 3, 4 => false
//@ run-call: eq_(uint256,uint256) 4, 4 => true

type Balance is uint256;

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
