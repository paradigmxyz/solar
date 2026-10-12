//@ codegen-matrix: standard
//@ run-call: crossWidthLocal => 0x0700
//@ run-call: crossWidthConstant => 0x0700
//@ run-call: crossWidthStructField => 0x0700
//@ run-call: crossWidthAssembly => 0x0700
//@ run-call: shiftRoundTrip => 0x00
//@ run-call: shiftRoundTripConstant => 0x00
//@ run-call: bitNotShift => 0x0f
//@ run-call: bitNotShiftConstant => 0x0f

contract FixedBytesConstantFoldWidth {
    struct S {
        bytes2 a;
        uint256 b;
    }

    bytes1 constant B1 = 0x07;
    bytes2 constant B2 = B1;
    bytes1 constant ROUND_TRIP = (B1 << 8) >> 8;
    bytes1 constant NOT_SHIFT = ~B1 >> 4;

    S s;

    function crossWidthLocal() external pure returns (bytes2) {
        bytes2 value = B2;
        return value;
    }

    function crossWidthConstant() external pure returns (bytes2) {
        return B2;
    }

    function crossWidthStructField() external returns (bytes2) {
        s = S(B2, 1);
        return s.a;
    }

    function crossWidthAssembly() external pure returns (bytes2 r) {
        assembly {
            r := B2
        }
    }

    function shiftRoundTrip() external pure returns (bytes1) {
        bytes1 value = (B1 << 8) >> 8;
        return value;
    }

    function shiftRoundTripConstant() external pure returns (bytes1) {
        return ROUND_TRIP;
    }

    function bitNotShift() external pure returns (bytes1) {
        bytes1 value = ~B1 >> 4;
        return value;
    }

    function bitNotShiftConstant() external pure returns (bytes1) {
        return NOT_SHIFT;
    }
}
