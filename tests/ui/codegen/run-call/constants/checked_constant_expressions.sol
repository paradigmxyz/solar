//@ codegen-matrix: standard
//@ run-call-fail: CheckedConstants::underflow => Panic(0x11)
//@ run-call: CheckedConstants::uncheckedNarrow => true
//@ run-call: CheckedConstants::narrowShift => true

contract CheckedConstants {
    uint256 constant A = 100;
    uint8 constant B = 250;
    uint8 constant ONE = 1;

    function underflow() external pure returns (uint256) {
        return A - 150;
    }

    function uncheckedNarrow() external pure returns (bool) {
        unchecked {
            return B + 10 == 4;
        }
    }

    function narrowShift() external pure returns (bool) {
        return ONE << 8 == 0;
    }
}
