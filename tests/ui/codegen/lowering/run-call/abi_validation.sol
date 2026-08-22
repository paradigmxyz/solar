//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: vBool false => false
//@ run-call: vUint8 7 => 7
//@ run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@ run-call-fail: 0x18a11c47
//@ run-call-fail: 0xd5f6949e

contract AbiValidation {
    function vBool(bool value) external pure returns (bool) {
        return value;
    }

    function vUint8(uint8 value) external pure returns (uint8) {
        return value;
    }
}
