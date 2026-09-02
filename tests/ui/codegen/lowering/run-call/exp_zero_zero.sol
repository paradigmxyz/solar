//@ codegen-matrix: standard
//@ run-call: literal => 1
// ported-from: test/libsolidity/semanticTests/expressions/exp_zero_literal.sol

contract ExpZeroZero {
    function literal() external pure returns (uint256) {
        return 0 ** 0;
    }
}
