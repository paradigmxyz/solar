//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: fAndRead() => 4
// ported-from: test/libsolidity/semanticTests/modifiers/return_in_modifier.sol

contract ModifierReturnInLoop {
    uint256 private x;

    modifier run() {
        for (uint256 i = 1; i < 10; ++i) {
            if (i == 5) return;
            _;
        }
    }

    function f() public run {
        uint256 k = x;
        uint256 t = k + 1;
        x = t;
    }

    function fAndRead() external returns (uint256) {
        f();
        return x;
    }
}
