//@ codegen-matrix: standard
//@ run-call: SignedLiteralComparison::literalLeft -100 => true
//@ run-call: SignedLiteralComparison::literalLeft 100 => false
//@ run-call: SignedLiteralComparison::signedBranch -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 9
//@ run-call: SignedLiteralComparison::signedBranch -1 => 9
//@ run-call: SignedLiteralComparison::signedBranch 0 => 9
//@ run-call: SignedLiteralComparison::signedBranch 30 => 9
//@ run-call: SignedLiteralComparison::signedBranch 31 => 9
//@ run-call: SignedLiteralComparison::signedBranch 32 => 7
//@ run-call: SignedLiteralComparison::signedBranch 57896044618658097711785492504343953926634992332820282019728792003956564819967 => 7
//@ run-call: SignedLiteralComparison::unsignedBranch 0 => 9
//@ run-call: SignedLiteralComparison::unsignedBranch 1 => 9
//@ run-call: SignedLiteralComparison::unsignedBranch 2 => 7
//@ run-call: SignedLiteralComparison::unsignedBranch 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 7

contract SignedLiteralComparison {
    function literalLeft(int256 value) external pure returns (bool) {
        return 0 > value;
    }
    function signedBranch(int256 value) external pure returns (uint256) {
        if (value > 31) return 7;
        return 9;
    }

    function unsignedBranch(uint256 value) external pure returns (uint256) {
        if (value > 1) return 7;
        return 9;
    }
}
