//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300 => true
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f380 => false
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f3 => false
//@ run-call: same 0x, 0x => true

// The comparison as the libraries write it: two hashes. It answers the same
// on every input anyone can produce, and what it states is that the hashes
// agree, which is a claim about `keccak256` and not about the bytes.
// CHECK-LABEL: fn @same
// CHECK: keccak256
contract Unsafe {
    function same(bytes memory a, bytes memory b) public pure returns (bool result) {
        assembly ("memory-safe") {
            result := eq(keccak256(add(a, 0x20), mload(a)), keccak256(add(b, 0x20), mload(b)))
        }
    }
}
