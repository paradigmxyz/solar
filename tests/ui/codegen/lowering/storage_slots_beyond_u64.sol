//@compile-flags: -Zcodegen --emit=bin-runtime

// EVM storage slots span the full 2^256 space, and a fixed array like
// `uint256[0xffffffffffffffff]` walks the layout cursor past 2^64, which
// overflowed the previous 64-bit slot arithmetic and aborted.
// Variables after the array live at slots above 2^64 and must still
// round-trip through their own locations.
contract StorageSlotsBeyondU64 {
    uint256 a;
    uint256[0xffffffffffffffff] big;
    uint256 tail;

    function roundTrip(uint256 i, uint256 v, uint256 t) external returns (uint256) {
        a = 7;
        big[i] = v;
        tail = t;
        return a + big[i] + tail;
    }
}
