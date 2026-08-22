//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: narrow() => 0x1234, 0x12345678, 0x123456780000
//@[gas] run-call: narrow() => 0x1234, 0x12345678, 0x123456780000
//@[size] run-call: narrow() => 0x1234, 0x12345678, 0x123456780000
//@[none] run-call: numeric() => 4660, 305419896, 20015998304256
//@[gas] run-call: numeric() => 4660, 305419896, 20015998304256
//@[size] run-call: numeric() => 4660, 305419896, 20015998304256

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
