//@ codegen-matrix: standard
//@ run-call: narrowRoundTrip => 0x00345678
//@ run-call: wideRoundTrip => 0x00000000000000000000000000000000000000000000000000000000000000ff
//@ run-call: compare => true
//@ run-call: widenedCompare => true

// The constant evaluator has no `bytesN` arithmetic, so it must not fold these with exact integer
// math, which would keep the bits that a shift moves out of the value and compare values of
// different widths without left-aligning them.
contract FixedBytesConstantArithmetic {
    bytes4 constant N = 0x12345678;
    bytes4 constant NARROW_ROUND_TRIP = (N << 8) >> 8;
    bytes32 constant W = 0xff000000000000000000000000000000000000000000000000000000000000ff;
    bytes32 constant WIDE_ROUND_TRIP = (W << 8) >> 8;
    bool constant COMPARE = N == 0x12345678;
    bytes8 constant N8 = 0x1234567800000000;
    bool constant WIDENED_COMPARE = N == N8;

    function narrowRoundTrip() external pure returns (bytes4) {
        return NARROW_ROUND_TRIP;
    }

    function wideRoundTrip() external pure returns (bytes32) {
        return WIDE_ROUND_TRIP;
    }

    function compare() external pure returns (bool) {
        return COMPARE;
    }

    function widenedCompare() external pure returns (bool) {
        return WIDENED_COMPARE;
    }
}
