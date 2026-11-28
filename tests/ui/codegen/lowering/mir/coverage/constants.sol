//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A constant that has no value at compile time is computed by a helper each time the code uses it,
// and the helper counts the decision in its initializer.
contract Other {}

contract Constants {
    uint256 constant C = uint256(keccak256(type(Other).creationCode)) > 1 ? 1 : 2;

    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: icall @constant_0
    // CHECK-LABEL: fn @constant_0(
    // CHECK-NEXT: bb0:
    // The initializer counts its start, its condition, and the path it takes.
    // CHECK-NEXT: coverage 2
    // CHECK: coverage 3
    // CHECK-NEXT: jumpi
    // CHECK: coverage 4
    function f() external pure returns (uint256) {
        return C;
    }
}
