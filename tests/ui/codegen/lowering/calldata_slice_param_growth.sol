//@ compile-flags: -O gas
//@ run-call: a(bytes,uint256) 0x0102030405, 7 => 14
//@ run-call: a(bytes,uint256) 0x01, 3 => 4
//@ run-call: b(bytes,uint256) 0x010203, 1 => 7

// A calldata-slice parameter that stays SSA is split into ptr and len physical
// parameters by slice lowering, widening the signature prefix after the body's
// frame-local addresses were baked. Those offsets must shift up with the
// prefix; leaving them would make the backend read the named-return slot as a
// parameter or return word. The recursive call keeps `r` in its frame slot
// across the barrier, and two callers keep `h` from inlining.
contract CalldataSliceParamGrowth {
    uint256 counter;

    function rec(uint256 n) internal returns (uint256) {
        if (n == 0) return 0;
        return n + rec(n - 1);
    }

    function a(bytes calldata d, uint256 x) external returns (uint256) {
        return h(d, x);
    }

    function b(bytes calldata d, uint256 x) external returns (uint256) {
        return h(d, x) + 1;
    }

    function h(bytes calldata s, uint256 x) internal returns (uint256 r) {
        r = x + 1;
        counter = rec(counter);
        if (s.length > 2) {
            r = r + s.length + uint8(s[0]);
        }
    }
}
