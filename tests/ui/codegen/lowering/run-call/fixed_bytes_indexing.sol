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
//@[none] run-call: at1(bytes1,uint256) 0xa5, 0 => 0xa5
//@[gas] run-call: at1(bytes1,uint256) 0xa5, 0 => 0xa5
//@[size] run-call: at1(bytes1,uint256) 0xa5, 0 => 0xa5
//@[none] run-call-fail: at1(bytes1,uint256) 0xa5, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[gas] run-call-fail: at1(bytes1,uint256) 0xa5, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[size] run-call-fail: at1(bytes1,uint256) 0xa5, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[none] run-call: at7(bytes7,uint256) 0x01020304050607, 0 => 0x01
//@[gas] run-call: at7(bytes7,uint256) 0x01020304050607, 0 => 0x01
//@[size] run-call: at7(bytes7,uint256) 0x01020304050607, 0 => 0x01
//@[none] run-call: at7(bytes7,uint256) 0x01020304050607, 3 => 0x04
//@[gas] run-call: at7(bytes7,uint256) 0x01020304050607, 3 => 0x04
//@[size] run-call: at7(bytes7,uint256) 0x01020304050607, 3 => 0x04
//@[none] run-call: at7(bytes7,uint256) 0x01020304050607, 6 => 0x07
//@[gas] run-call: at7(bytes7,uint256) 0x01020304050607, 6 => 0x07
//@[size] run-call: at7(bytes7,uint256) 0x01020304050607, 6 => 0x07
//@[none] run-call-fail: at7(bytes7,uint256) 0x01020304050607, 7 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[gas] run-call-fail: at7(bytes7,uint256) 0x01020304050607, 7 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[size] run-call-fail: at7(bytes7,uint256) 0x01020304050607, 7 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[none] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0 => 0x00
//@[gas] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0 => 0x00
//@[size] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0 => 0x00
//@[none] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31 => 0x1f
//@[gas] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31 => 0x1f
//@[size] run-call: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31 => 0x1f
//@[none] run-call-fail: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 32 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[gas] run-call-fail: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 32 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[size] run-call-fail: at32(bytes32,uint256) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 32 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

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
