//@ codegen-matrix: standard
//@ run-call: isMagic 0x12345678 => true
//@ run-call: isMagic 0x12345679 => false
//@ run-call-fail: 0xafd7207112345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: isSeven 7 => true
//@ run-call-fail: 0x0148adbd0000000000000000000000000000000000000000000000000000000000000107

// ABI coder v1 cleans a dirty `bytes4` or `uint8` argument where v2 reverts; we compile this
// as ABI coder v2, so every argument is validated before the callee trusts it as clean.
pragma abicoder v1; //~ WARN: ABI coder v1 is not supported

contract AbicoderV1DirtyCalldata {
    function isMagic(bytes4 id) public pure returns (bool) {
        return id == 0x12345678;
    }

    function isSeven(uint8 value) public pure returns (bool) {
        return value == 7;
    }
}
