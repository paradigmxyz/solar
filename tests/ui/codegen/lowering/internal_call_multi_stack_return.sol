//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: pair 2 => 209, 364
//@[run] run-call: triple 2 => 209, 364, 901
//@[run] run-call: sumPair 2 => 573
//@[run] run-call: sumTriple 2 => 1474

contract InternalCallMultiStackReturn {
    // A two-word stack return rotates the hidden return label above both results.
    // CHECK: swap1
    // CHECK-NEXT: swap2
    // CHECK-NEXT: jump
    function pair(uint256 x) external pure returns (uint256, uint256) {
        return pairHelper(x);
    }

    function sumPair(uint256 x) external pure returns (uint256) {
        (uint256 a, uint256 b) = pairHelper(x);
        return a + b;
    }

    function pairHelper(uint256 x) internal pure returns (uint256 a, uint256 b) {
        unchecked {
            a = x + 1;
            a *= 3;
            a ^= 5;
            a += 7;
            a *= 11;

            b = x + 2;
            b *= 5;
            b ^= 7;
            b += 9;
            b *= 13;
        }
    }

    // Three results exercise the complete SWAP1..SWAP3 return-label rotation.
    // CHECK: swap1
    // CHECK-NEXT: swap2
    // CHECK-NEXT: swap3
    // CHECK-NEXT: jump
    function triple(uint256 x) external pure returns (uint256, uint256, uint256) {
        return tripleHelper(x);
    }

    function sumTriple(uint256 x) external pure returns (uint256) {
        (uint256 a, uint256 b, uint256 c) = tripleHelper(x);
        return a + b + c;
    }

    function tripleHelper(uint256 x)
        internal
        pure
        returns (uint256 a, uint256 b, uint256 c)
    {
        unchecked {
            a = x + 1;
            a *= 3;
            a ^= 5;
            a += 7;
            a *= 11;

            b = x + 2;
            b *= 5;
            b ^= 7;
            b += 9;
            b *= 13;

            c = x + 3;
            c *= 7;
            c ^= 11;
            c += 13;
            c *= 17;
        }
    }
}
