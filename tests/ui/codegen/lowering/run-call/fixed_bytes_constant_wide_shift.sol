//@ codegen-matrix: standard
//@ run-call: shiftOut => 0x000000000000000000000000000000000000000000000000000000000000ff00
//@ run-call: shiftRoundTrip => 0x00000000000000000000000000000000000000000000000000000000000000ff
//@ run-call: shiftRoundTripLocal => 0x00000000000000000000000000000000000000000000000000000000000000ff

// Shifting a `bytes32` constant drops the bits that leave the word, however wide literal
// arithmetic may grow.
contract FixedBytesConstantWideShift {
    bytes32 constant B = 0xff000000000000000000000000000000000000000000000000000000000000ff;
    bytes32 constant SHIFT_OUT = B << 8;
    bytes32 constant ROUND_TRIP = (B << 8) >> 8;

    function shiftOut() external pure returns (bytes32) {
        return SHIFT_OUT;
    }

    function shiftRoundTrip() external pure returns (bytes32) {
        return ROUND_TRIP;
    }

    function shiftRoundTripLocal() external pure returns (bytes32) {
        bytes32 value = (B << 8) >> 8;
        return value;
    }
}
