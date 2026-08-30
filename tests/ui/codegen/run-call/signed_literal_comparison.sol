//@ codegen-matrix: standard
//@ run-call: SignedLiteralComparison::literalLeft(int256) -100 => true
//@ run-call: SignedLiteralComparison::literalLeft(int256) 100 => false

contract SignedLiteralComparison {
    function literalLeft(int256 value) external pure returns (bool) {
        return 0 > value;
    }
}
