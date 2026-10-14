//@ codegen-matrix: standard
//@ run-call: f => 11, 10
// ported-from: test/libsolidity/semanticTests/constantEvaluator/negative_fractional_mod.sol

contract NegativeFractionalMod {
    // The remainder of literal arithmetic truncates like integer division, so `-5.2 % 3` is
    // `-2.2`.
    function f() public pure returns (int256, int256) {
        int256 x = int256((-(-5.2 % 3)) * 5);
        int256 t = 5;
        return (x, (-(-t % 3)) * 5);
    }
}
