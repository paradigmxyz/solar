//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: vBool false => false
//@ run-call: vUint8 7 => 7
//@ run-call: vAddress 0x000000000000000000000000000000000000beef => 0x000000000000000000000000000000000000beef
//@ run-call: dirtyBytes1(bytes32) 0x0000000000000000000000000000000000000000000000000000000000008000 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0x18a11c470000000000000000000000000000000000000000000000000000000000000002
//@ run-call-fail: 0x18a11c47
//@ run-call-fail: 0xd5f6949e
//@ run-call-fail: 0x23347413000000000000000000000001000000000000000000000000000000000000beef
// ported-from: test/libsolidity/semanticTests/abicoder/cleanup/bytesx_v2.sol

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

    function dirtyBytes1(bytes32 value) external view returns (bytes32) {
        bytes1 narrowed;
        assembly {
            narrowed := value
        }
        return this.bytes1Target(narrowed);
    }

    function bytes1Target(bytes1 value) external pure returns (bytes32) {
        return value;
    }
}
