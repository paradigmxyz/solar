//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 0x6dddb956123400000000000000000000000000000000000000000000000000000000000061620000000000000000000000000000000000000000000000000000000000001234000000000000000000000000000000000000000000000000000000000000
//@ run-call: f2() => 0x2a62baf500000000000000000000000000000000000000000000000000000000000012340000000000000000000000000000000000000000000000000000000000001234
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_uint_bytes.sol

contract AbiEncodeCallUintBytes {
    function g(bytes2, bytes2, bytes2) public pure {}

    function h(uint16, uint16) public pure {}

    function f() public view returns (bytes memory) {
        uint16 value = 0x1234;
        return abi.encodeCall(this.g, (0x1234, "ab", bytes2(value)));
    }

    function f2() public view returns (bytes memory) {
        bytes2 value = 0x1234;
        return abi.encodeCall(this.h, (0x1234, uint16(value)));
    }
}
