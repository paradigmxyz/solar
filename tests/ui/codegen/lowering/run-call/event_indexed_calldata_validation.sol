//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: emitValues [1] => 1
//@[none, gas, size] run-call: 0x797a5ca9000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001 => 0x0000000000000000000000000000000000000000000000000000000000000001
//@[none, gas, size] run-call-fail: 0x797a5ca9000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000010101

contract EventIndexedCalldataValidation {
    event IndexedNarrow(uint8[] indexed values);

    function emitValues(uint8[] calldata values) external returns (uint256) {
        emit IndexedNarrow(values);
        return values.length;
    }
}
