//@ codegen-matrix: standard
//@ run-call: calldataHashes(bytes) 0x616263 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad, 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc, 0
//@ run-call: calldataHashes(bytes) 0x => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0x9c1185a5c5e9fc54612808977ee8f548b2258d31, 0
//@ run-call: calldataHashes(bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f60616263 => 0xbce0aff19cf5aa6a7469a30d61d04e4376e4bbf6381052ee9e7f33925c954d52, 0x8ae5d2e6b1f3a514257f2469b637454931844aeb, 0
//@ run-call: sliceHashes(bytes,uint256,uint256) 0x0061626300, 1, 4 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad, 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@ run-call: sliceHashes(bytes,uint256,uint256) 0x616263, 1, 3 => 0x1e0bbd6c686ba050b8eb03ffeedc64fdc9d80947fce821abbe5d6dc8d252c5ac, 0x052d7b0495b75a438ea007d80c4925839bf8e5ec
//@ run-call: sliceHashes(bytes,uint256,uint256) 0x616263, 2, 2 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@ run-call: memoryHashes(bytes) 0x616263 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad, 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@ run-call: memoryHashes(bytes) 0x => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@ run-call: forgedHashes(uint256,uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@ run-call: forgedHashes(uint256,uint256) 0x100000000000000000000000000000000, 32 => 0x66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925, 0xd1a70126ff7a149ca6f9b638db084480440ff842
//@ run-call: recover(bytes32,uint8,bytes32,bytes32) 0x0000000000000000000000000000000000000000000000000000000000000001, 28, 0x6673ffad2147741f04772b6f921f0ba6af0c1e77fc439e65c36dedf4092e8898, 0x4c1a971652e0ada880120ef8025e709fff2080c4a39aae068d12eed009b68c89 => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: recover(bytes32,uint8,bytes32,bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000, 0, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000

// Calldata input is staged past the free memory pointer without reserving it, memory input is
// read where it lies, and a nonempty read beyond the calldata is zero-padded. Nothing here moves
// the free memory pointer.
contract PrecompileInput {
    function calldataHashes(bytes calldata input) external pure returns (bytes32, bytes20, uint256 growth) {
        uint256 before_;
        assembly { before_ := mload(64) }
        (bytes32 digest, bytes20 ripemd) = (sha256(input), ripemd160(input));
        assembly { growth := sub(mload(64), before_) }
        return (digest, ripemd, growth);
    }

    function sliceHashes(bytes calldata input, uint256 start, uint256 end) external pure returns (bytes32, bytes20) {
        return (sha256(input[start:end]), ripemd160(input[start:end]));
    }

    function memoryHashes(bytes memory input) external pure returns (bytes32, bytes20) {
        return (sha256(input), ripemd160(input));
    }

    function forgedHashes(uint256 offset, uint256 length) external pure returns (bytes32, bytes20) {
        bytes calldata input;
        assembly {
            input.offset := offset
            input.length := length
        }
        return (sha256(input), ripemd160(input));
    }

    function recover(bytes32 hash, uint8 v, bytes32 r, bytes32 s) external pure returns (address) {
        return ecrecover(hash, v, r, s);
    }
}
