//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: threeItems 7, 0x0000000000000000000000000000000000000001 => 0xbe7a03c349b548ca89e68a0e7fd4e24363212cc4240c4fa749d9c35b2919dfa2
//@ run-call: twoItems 7, 0x0000000000000000000000000000000000000001 => 0xbe7a03c349b548ca89e68a0e7fd4e24363212cc4240c4fa749d9c35b2919dfa2
//@ run-call: tooLong 1, 2, 3 => 0x37a684a9f85be418aa77a0b9d67dcf8b98300298c19aab61dcc750f7d20dba52

// A packed hash is built in the scratch space when its stores end by 0x40. Narrow items that
// share one word store count as that store: a word, then a `uint32` and an address in one
// word, is 56 bytes written by two stores, as when the two narrow items are one `uint192`.
contract PackedHashScratch {
    // CHECK-LABEL: fn @threeItems(
    // CHECK-NOT: mload 64
    // CHECK: keccak256 0, 56
    function threeItems(uint256 role, address holder) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(role, uint32(0xee9853bb), holder));
    }

    // CHECK-LABEL: fn @twoItems(
    // CHECK-NOT: mload 64
    // CHECK: keccak256 0, 56
    function twoItems(uint256 role, address holder) external pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(role, uint192((uint256(0xee9853bb) << 160) | uint160(holder)))
        );
    }

    // A third store would reach past 0x40, so the bytes go to the free memory pointer.
    // CHECK-LABEL: fn @tooLong(
    // CHECK: mload 64
    // CHECK: keccak256
    function tooLong(uint256 a, uint256 b, uint32 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }
}
