//@ revisions: ir run size
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --implicit-check-not=mload
//@[run] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: pair 2 => 209, 364
//@ run-call: triple 2 => 209, 364, 901
//@ run-call: six 2 => 209, 364, 901, 1824, 2139, 5162
//@ run-call: sumPair 2 => 573
//@ run-call: sumTriple 2 => 1474
//@ run-call: sumSix 2 => 10599

contract ICallMultiStackReturn {
    // Multi-word results return on the stack, with the return address moved on top of
    // them; no caller reads results back from memory.
    // CHECK-LABEL: @module ICallMultiStackReturn_runtime
    // A two-word return swaps the return address above both results.
    // CHECK: mul{{[[:space:]]+}}swap 1{{[[:space:]]+}}jump{{$}}
    // Three results reach their order and the return address in three swaps.
    // CHECK: swap 1{{[[:space:]]+}}swap 3{{[[:space:]]+}}swap 2{{[[:space:]]+}}jump{{$}}
    // Six results are already live on the stack; the return reuses those words in place
    // instead of duplicating the tuple.
    // CHECK: swap 6{{[[:space:]]+}}swap 2{{[[:space:]]+}}swap 4{{[[:space:]]+}}swap 6{{[[:space:]]+}}jump{{$}}
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
