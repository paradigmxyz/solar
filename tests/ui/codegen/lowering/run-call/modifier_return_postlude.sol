//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
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
