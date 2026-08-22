//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f 9 => [0, 0, 0, 0, 0]
//@[none, gas, size] run-call: f 10 => [0, 0, 3, 0, 0]
// ported-from: test/libsolidity/semanticTests/modifiers/modifier_init_return.sol

contract ModifierInitReturn {
    modifier onlyWhenLarge(bool condition) {
        if (condition) _;
    }

    function f(uint256 x)
        external
        pure
        onlyWhenLarge(x >= 10)
        returns (uint256[5] memory r)
    {
        r[2] = 3;
    }
}
