//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: header 0xa9059cbb00000000000000000000000000000000000000000000000000000000000000ff => 0xa9059cbb, 0xe08ec2af2cfc251225e1968fd6ca21e4044f129bffa95bac3503be8bdb30a367
//@ run-call: header 0xa9059cbb => 0xa9059cbb, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470

// The same views written the way the assembly libraries write them: pointer
// arithmetic into the packet, with no range check. On valid input the two
// agree. A packet shorter than its header underflows the body's length, and
// the hash then runs out of gas instead of failing cleanly.
// CHECK-LABEL: fn @header
// CHECK: keccak256
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// CHECK: returndata
contract Unsafe {
    function header(bytes memory packet) public pure returns (bytes4 selector, bytes32 bodyHash) {
        assembly ("memory-safe") {
            selector := and(mload(add(packet, 0x20)), shl(224, 0xffffffff))
            bodyHash := keccak256(add(packet, 0x24), sub(mload(packet), 4))
        }
    }
}
