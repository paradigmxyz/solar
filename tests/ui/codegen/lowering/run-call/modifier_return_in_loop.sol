//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: fAndRead() => 4
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
