//@ codegen-matrix: standard
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: hash(bytes,uint256) 0x616263, 100 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad, 0
//@ run-call: hash(bytes,uint256) 0x, 100 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0
//@ run-call: hashSlice(bytes,uint256,uint256) 0x0061626300, 1, 4 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@ run-call: forged(uint256,uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@ run-call: forged(uint256,uint256) 0x100000000000000000000000000000000, 32 => 0x66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925, 0xd1a70126ff7a149ca6f9b638db084480440ff842
//@ run-call: ripemd(bytes) 0x616263 => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc, 0

contract CalldataPrecompileInput {
    function hash(bytes calldata input, uint256 n) external pure returns (bytes32 result, uint256 growth) {
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        for (uint256 i; i < n; ++i) result = sha256(input);
        assembly { growth := sub(mload(64), beforePointer) }
    }

    function hashSlice(bytes calldata input, uint256 start, uint256 end) external pure returns (bytes32) {
        return sha256(input[start:end]);
    }

    function ripemd(bytes calldata input) external pure returns (bytes20 result, uint256 growth) {
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        result = ripemd160(input);
        assembly { growth := sub(mload(64), beforePointer) }
    }

    // Zero-length copies ignore their offsets; nonempty reads beyond calldata are zero-padded.
    function forged(uint256 offset, uint256 length) external pure returns (bytes32, bytes20) {
        bytes calldata input;
        assembly {
            input.offset := offset
            input.length := length
        }
        return (sha256(input), ripemd160(input));
    }
}
