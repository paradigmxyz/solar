//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 2, 2, 2, 2
// ported-from: test/libsolidity/semanticTests/constantEvaluator/rounding.sol

contract ConstantRoundingArrayLength {
    int256 constant a = 7;
    int256 constant b = 3;
    int256 constant c = a / b;
    int256 constant d = (-a) / b;

    // A signed constant is a valid array length as long as its value is a positive integer.
    function f() public pure returns (uint256, int256, uint256, int256) {
        uint256[c] memory x;
        uint256[-d] memory y;
        return (x.length, c, y.length, -d);
    }
}
