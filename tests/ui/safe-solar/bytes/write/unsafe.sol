//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: patch 0xaaaaaaaaaaaaaaaa, 0x01020304 => 0xaa01020304aaaaaa
//@ run-call: word 0x1111111111111111111111111111111111111111111111111111111111111111, 3735928559 => 0x00000000000000000000000000000000000000000000000000000000deadbeef
//@ run-call: patch 0xaaaaaaaa, 0x01020304 => 0xaa010203

// The same writes in assembly. On valid input they agree. Writing four bytes
// at offset one of a four-byte buffer does not fail: three land inside, and
// the fourth lands in whatever memory follows the buffer.
// CHECK-LABEL: fn @patch
// CHECK: mload
// CHECK: mstore
contract Unsafe {
    function patch(bytes memory buffer, bytes4 value) public pure returns (bytes memory) {
        assembly ("memory-safe") {
            let p := add(buffer, 0x21)
            let mask := shl(224, 0xffffffff)
            mstore(p, or(and(mload(p), not(mask)), and(value, mask)))
        }
        return buffer;
    }

    function word(bytes memory buffer, uint256 value) public pure returns (bytes memory) {
        assembly ("memory-safe") {
            mstore(add(buffer, 0x20), value)
        }
        return buffer;
    }
}
