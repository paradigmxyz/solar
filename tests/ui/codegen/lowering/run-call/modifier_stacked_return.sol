//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f => 42
//@[none, gas, size] run-call: fAndRead => 4
// ported-from: test/libsolidity/semanticTests/modifiers/stacked_return_with_modifiers.sol

contract ModifierStackedReturn {
    uint256 private x;

    modifier m() {
        for (uint256 i; i < 10; ++i) {
            _;
            ++x;
            return;
        }
    }

    function f() public m m m returns (uint256) {
        for (uint256 i; i < 10; ++i) {
            ++x;
            return 42;
        }
    }

    function fAndRead() external returns (uint256) {
        f();
        return x;
    }
}
