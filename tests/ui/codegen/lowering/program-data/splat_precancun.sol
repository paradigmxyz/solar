//@ codegen-matrix: standard optimized
//@ compile-flags: --evm-version paris
//@[optimized] compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@[optimized] filecheck:
//@ run-call: hash => 0x0ef44b5473a7da194a6867b15fcc63e6da82f43c1586e43af0120a1687530c4d
//@ run-call: paddingIsZero => true
//@ run-call: shortHash => 0xc224daeb584cbf83e794907fde0250faac206e7c1360b6581fc5e906a30e72fd
//@ run-call: fullWordsHash => 0x33151a08ebcfee753117f4d08eda9c643dca50e49f121bd38b38e68e28131688

contract C {
    function data() internal pure returns (bytes memory) {
        return hex"117cb4bd2c12a41e06e757fff19d023f5d98e82e8a09739ad70f95c7c805e1df117cb4bd2c12a41e06e757fff19d023f5d98e82e8a09739ad70f95c7c805e1df117cb4bd2c12a41e06e757fff19d023f5d98e82e8a09739ad70f95c7c805e1df117cb4bd2c12a41e06e757fff19d023f5d98e82e8a09739ad70f95c7c805e1df117cb4bd2c12a41e06e757fff19d023f5d98e82e8a09739ad70f95c7c805e1df11";
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
