//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: narrowRoundTrip => 0x00345678
//@ run-call: wideRoundTrip => 0x00000000000000000000000000000000000000000000000000000000000000ff
//@ run-call: compare => true
//@ run-call: widenedCompare => true

// Constant evaluation folds `bytesN` operations in the width of their type, so shifts drop the bits
// they move out of the value, and values of different widths compare left-aligned.
contract FixedBytesConstantArithmetic {
    bytes4 constant N = 0x12345678;
    bytes4 constant NARROW_ROUND_TRIP = (N << 8) >> 8;
    bytes32 constant W = 0xff000000000000000000000000000000000000000000000000000000000000ff;
    bytes32 constant WIDE_ROUND_TRIP = (W << 8) >> 8;
    bool constant COMPARE = N == 0x12345678;
    bytes8 constant N8 = 0x1234567800000000;
    bool constant WIDENED_COMPARE = N == N8;

    // CHECK-LABEL: fn @narrowRoundTrip()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x34567800000000000000000000000000000000000000000000000000000000
    function narrowRoundTrip() external pure returns (bytes4) {
        return NARROW_ROUND_TRIP;
    }

    // CHECK-LABEL: fn @wideRoundTrip()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 255
    function wideRoundTrip() external pure returns (bytes32) {
        return WIDE_ROUND_TRIP;
    }

    // CHECK-LABEL: fn @compare()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret true
    function compare() external pure returns (bool) {
        return COMPARE;
    }

    // CHECK-LABEL: fn @widenedCompare()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret true
    function widenedCompare() external pure returns (bool) {
        return WIDENED_COMPARE;
    }
}
