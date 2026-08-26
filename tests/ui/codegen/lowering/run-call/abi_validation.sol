//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: vBool false => false
//@ run-call: vUint8 7 => 7
//@ run-call: vAddress 0x000000000000000000000000000000000000beef => 0x000000000000000000000000000000000000beef
//@ run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@ run-call-fail: 0x18a11c47
//@ run-call-fail: 0xd5f6949e
//@ run-call-fail: 0x23347413000000000000000000000001000000000000000000000000000000000000beef

contract AbiValidation {
    function vBool(bool value) external pure returns (bool) {
        return value;
    }

    function vUint8(uint8 value) external pure returns (uint8) {
        return value;
    }

    function vAddress(address value) external view returns (address) {
        return this.vAddressTarget(value);
    }

    function vAddressTarget(address value) external pure returns (address) {
        return value;
    }
}
