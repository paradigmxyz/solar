//@ compile-flags: -O gas
//@ run-call: a(bytes,uint256) 0x0102030405, 2 => 5102
//@ run-call: a(bytes,uint256) 0x0102030405, 4 => 4104
//@ run-call: b(bytes,uint256) 0x01, 0 => 1201

// A rebindable calldata-slice parameter followed by more parameters keeps its
// two-word frame slot. The entry stores of that slot must resolve against the
// complete signature prefix, exactly like the body's reads: baking them while
// later parameters were still unregistered stored the slice below the locals
// region — clobbering parameter and return slots — and the body then read the
// slot from uninitialized memory on every path that does not reassign first.
// Two callers keep `h` from inlining so the frame convention is exercised.
contract CalldataSliceParamSlot {
    function a(bytes calldata d, uint256 x) external pure returns (uint256) {
        return h(d, x, 100);
    }

    function b(bytes calldata d, uint256 x) external pure returns (uint256) {
        return h(d, x, 200) + 1;
    }

    function h(bytes calldata s, uint256 x, uint256 y) internal pure returns (uint256) {
        if (x > 3) {
            s = s[1:];
        }
        return s.length * 1000 + x + y;
    }
}
