//@ codegen-matrix: standard
//@ run-call: shiftOut => 0x000000000000000000000000000000000000000000000000000000000000ff00
//@ run-call: shiftRoundTrip => 0x00000000000000000000000000000000000000000000000000000000000000ff

// Exact literal arithmetic must not fold shifts of `bytesN` constants: `B << 8` is wider than the
// EVM word, and `(B << 8) >> 8` would keep the bits that the first shift drops.
contract FixedBytesConstantWideShift {
    bytes32 constant B = 0xff000000000000000000000000000000000000000000000000000000000000ff;
    bytes32 constant SHIFT_OUT = B << 8;
    bytes32 constant SHIFT_ROUND_TRIP = (B << 8) >> 8;

    function shiftOut() external pure returns (bytes32) {
        return SHIFT_OUT;
    }

    function shiftRoundTrip() external pure returns (bytes32) {
        return SHIFT_ROUND_TRIP;
    }
}
