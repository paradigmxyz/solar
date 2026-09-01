//@ revisions: ir run size
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[run] run-call: pair 2 => 209, 364
//@[run] run-call: triple 2 => 209, 364, 901
//@[run] run-call: six 2 => 209, 364, 901, 1824, 2139, 5162
//@[run] run-call: sumPair 2 => 573
//@[run] run-call: sumTriple 2 => 1474
//@[run] run-call: sumSix 2 => 10599
//@[size] run-call: pair 2 => 209, 364
//@[size] run-call: triple 2 => 209, 364, 901
//@[size] run-call: six 2 => 209, 364, 901, 1824, 2139, 5162
//@[size] run-call: sumPair 2 => 573
//@[size] run-call: sumTriple 2 => 1474
//@[size] run-call: sumSix 2 => 10599

contract InternalCallMultiStackReturn {
    // A two-word stack return rotates the hidden return label above both results.
    // CHECK-LABEL: @module InternalCallMultiStackReturn
    // CHECK: push 256
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[PAIR_RETURN:bb[0-9]+]]
    // CHECK: jump [[PAIR_HELPER:bb[0-9]+]]
    // CHECK: [[PAIR_HELPER]]:
    // CHECK: swap2
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
    // CHECK: push 320
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[TRIPLE_RETURN:bb[0-9]+]]
    // CHECK: jump [[TRIPLE_HELPER:bb[0-9]+]]
    // CHECK: [[TRIPLE_HELPER]]:
    // CHECK: swap3
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

    // Six results exercise a return whose values are already live on the physical stack. The
    // return shuffler must reuse those words instead of duplicating the entire tuple beyond its
    // requested layout.
    // CHECK: push 512
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[SIX_RETURN:bb[0-9]+]]
    // CHECK: jump [[SIX_HELPER:bb[0-9]+]]
    // CHECK: [[SIX_HELPER]]:
    // CHECK: swap2
    // CHECK-NEXT: swap3
    // CHECK-NEXT: swap4
    // CHECK-NEXT: swap5
    // CHECK-NEXT: swap6
    // CHECK-NEXT: jump
    function six(uint256 x)
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return sixHelper(x);
    }

    function sumSix(uint256 x) external pure returns (uint256) {
        (uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f) = sixHelper(x);
        return a + b + c + d + e + f;
    }

    function sixHelper(uint256 x)
        internal
        pure
        returns (uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
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

            d = x + 4;
            d *= 11;
            d ^= 13;
            d += 17;
            d *= 19;

            e = x + 5;
            e *= 13;
            e ^= 17;
            e += 19;
            e *= 23;

            f = x + 6;
            f *= 17;
            f ^= 19;
            f += 23;
            f *= 29;
        }
    }
}
