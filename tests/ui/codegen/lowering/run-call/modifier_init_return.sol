//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f 9 => [0, 0, 0, 0, 0]
//@ run-call: f 10 => [0, 0, 3, 0, 0]
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
