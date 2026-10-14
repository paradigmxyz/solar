//@ codegen-matrix: standard
//@ run-call: widened => 0x12340000
//@ run-call: combined => 0x12345678
//@ run-call: compared => true
//@ run-call: selected => 0x12340000

// `bytesN` values are left-aligned, so constants of different sizes convert like at runtime.
contract FixedBytesConstantWidths {
    bytes2 constant Y = 0x1234;
    bytes4 constant X = 0x12345678;
    bytes4 constant Z = 0x12340000;
    bytes4 constant YW = Y;
    bytes4 constant XY = X | Y;
    bool constant EQ = Y == Z;

    function widened() external pure returns (bytes4) {
        return YW;
    }

    function combined() external pure returns (bytes4) {
        return XY;
    }

    function compared() external pure returns (bool) {
        return EQ;
    }

    function selected() external pure returns (bytes4) {
        bytes4 value = false ? X : Y;
        return value;
    }
}
