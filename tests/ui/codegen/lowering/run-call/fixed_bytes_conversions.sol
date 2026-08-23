//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: narrow() => 0x1234, 0x12345678, 0x123456780000
//@ run-call: numeric() => 4660, 305419896, 20015998304256

contract FixedBytesConversions {
    function narrow() external pure returns (bytes2, bytes4, bytes6) {
        bytes4 value = 0x12345678;
        return (bytes2(value), value, bytes6(value));
    }

    function numeric() external pure returns (uint16, uint32, uint48) {
        bytes4 value = 0x12345678;
        return (uint16(bytes2(value)), uint32(value), uint48(bytes6(value)));
    }
}
