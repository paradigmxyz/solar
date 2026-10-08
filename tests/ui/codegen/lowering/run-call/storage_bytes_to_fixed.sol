//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: short => 0x11223300
//@ run-call: long => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20

contract StorageBytesToFixed {
    bytes stored;

    function short() external returns (bytes4) {
        stored = hex"112233";
        return bytes4(stored);
    }

    function long() external returns (bytes32) {
        stored = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        return bytes32(stored);
    }
}
