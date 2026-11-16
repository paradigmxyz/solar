//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: f 1, false => 0xc8c80166610cedbdd21ec5dd1e8ab4e9eaa4a94ee980d51d8e685dbd1ddc2017
//@ run-call: f 1, true => 0xc8c80166610cedbdd21ec5dd1e8ab4e9eaa4a94ee980d51d8e685dbd1ddc2017

// Assembly steps back 4 GiB from the free memory pointer, then forward to just below it. The
// heap prefix guard reserves nothing for a step back that far, so the spill hazard analysis must
// not trust the writes through it either: they land on the words below the heap, where the
// eighteen hashes would otherwise spill, and the result must still be their XOR.
contract HeapPrefixFarStep {
    function f(uint256 s, bool b) external pure returns (bytes32) {
        bytes32 a1 = keccak256(abi.encode(s + 1)); bytes32 a2 = keccak256(abi.encode(s + 2));
        bytes32 a3 = keccak256(abi.encode(s + 3)); bytes32 a4 = keccak256(abi.encode(s + 4));
        bytes32 a5 = keccak256(abi.encode(s + 5)); bytes32 a6 = keccak256(abi.encode(s + 6));
        bytes32 a7 = keccak256(abi.encode(s + 7)); bytes32 a8 = keccak256(abi.encode(s + 8));
        bytes32 a9 = keccak256(abi.encode(s + 9)); bytes32 a10 = keccak256(abi.encode(s + 10));
        bytes32 a11 = keccak256(abi.encode(s + 11)); bytes32 a12 = keccak256(abi.encode(s + 12));
        bytes32 a13 = keccak256(abi.encode(s + 13)); bytes32 a14 = keccak256(abi.encode(s + 14));
        bytes32 a15 = keccak256(abi.encode(s + 15)); bytes32 a16 = keccak256(abi.encode(s + 16));
        bytes32 a17 = keccak256(abi.encode(s + 17)); bytes32 a18 = keccak256(abi.encode(s + 18));
        assembly {
            let p := mload(0x40)
            let q := sub(p, 0x100000000)
            if b { q := sub(p, 0x100000020) }
            mstore(add(q, 0xffffffe0), 0xdead)
            mstore(add(q, 0xffffffc0), 0xdead)
        }
        return a1 ^ a2 ^ a3 ^ a4 ^ a5 ^ a6 ^ a7 ^ a8 ^ a9 ^ a10 ^ a11 ^ a12 ^ a13 ^ a14 ^ a15
            ^ a16 ^ a17 ^ a18;
    }
}
