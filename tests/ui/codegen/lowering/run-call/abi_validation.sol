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
//@[none] run-call: vBool false => false
//@[gas] run-call: vBool false => false
//@[size] run-call: vBool false => false
//@[none] run-call: vUint8 7 => 7
//@[gas] run-call: vUint8 7 => 7
//@[size] run-call: vUint8 7 => 7
//@[none] run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@[gas] run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@[size] run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@[none] run-call-fail: 0x18a11c47
//@[gas] run-call-fail: 0x18a11c47
//@[size] run-call-fail: 0x18a11c47
//@[none] run-call-fail: 0xd5f6949e
//@[gas] run-call-fail: 0xd5f6949e
//@[size] run-call-fail: 0xd5f6949e

contract AbiValidation {
    function vBool(bool value) external pure returns (bool) {
        return value;
    }

    function vUint8(uint8 value) external pure returns (uint8) {
        return value;
    }
}
