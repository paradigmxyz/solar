//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f => 42
//@ run-call: fAndRead => 4
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
