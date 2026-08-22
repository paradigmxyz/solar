//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f() => 2
//@ run-call: fThenRead() => 9
// ported-from: test/libsolidity/semanticTests/modifiers/return_does_not_skip_modifier.sol

contract ModifierReturnPostlude {
    uint256 private x;

    modifier setsX() {
        _;
        x = 9;
    }

    function f() public setsX returns (uint256) {
        return 2;
    }

    function fThenRead() external returns (uint256) {
        f();
        return x;
    }
}
