//@ codegen-matrix: standard optimized
//@ compile-flags: --evm-version paris
//@[optimized] compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@[optimized] filecheck:
//@ run-call: hash => 0x04cea613ff619f915864b8d95704e4704cdb3947a5b6f98be176f30db7333a2f
//@ run-call: paddingIsZero => true
//@ run-call: shortHash => 0xc224daeb584cbf83e794907fde0250faac206e7c1360b6581fc5e906a30e72fd
//@ run-call: fullWordsHash => 0x33151a08ebcfee753117f4d08eda9c643dca50e49f121bd38b38e68e28131688

contract C {
    function data() internal pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff0011";
    }

    function hash() external pure returns (bytes32) {
        return keccak256(data());
    }

    function paddingIsZero() external pure returns (bool result) {
        bytes memory value = data();
        assembly {
            result := iszero(shr(8, mload(add(add(value, 32), mload(value)))))
        }
    }

    // CHECK-LABEL: fn @shortHash
    // CHECK-NOT: icall @mcopy_words
    // CHECK: return
    function shortHash() external pure returns (bytes32) {
        bytes memory value = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2001";
        return keccak256(value);
    }

    // CHECK-LABEL: fn @fullWordsHash
    // CHECK-NOT: datacopy
    // CHECK: return
    function fullWordsHash() external pure returns (bytes32) {
        bytes memory value = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f200102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f200102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        return keccak256(value);
    }
}
