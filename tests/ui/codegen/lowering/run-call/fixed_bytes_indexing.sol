//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: at1(bytes1,uint256) 0xa5, 0 => 0xa5
//@ run-call-fail: at1(bytes1,uint256) 0xa5, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: at7(bytes7,uint256) 0x01020304050607, 0 => 0x01
//@ run-call: at7(bytes7,uint256) 0x01020304050607, 3 => 0x04
//@ run-call: at7(bytes7,uint256) 0x01020304050607, 6 => 0x07
//@ run-call-fail: at7(bytes7,uint256) 0x01020304050607, 7 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0 => 0x00
//@ run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31 => 0x1f
//@ run-call-fail: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 32 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract C {
    function at1(bytes1 value, uint256 index) external pure returns (bytes1) {
        return value[index];
    }

    function at7(bytes7 value, uint256 index) external pure returns (bytes1) {
        return value[index];
    }

    function at32(bytes32 value, uint256 index) external pure returns (bytes1) {
        return value[index];
    }
}
