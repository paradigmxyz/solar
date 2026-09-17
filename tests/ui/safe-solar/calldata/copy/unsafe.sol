//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: part 0x0102030405, 1, 3 => 0x020304
//@ run-call: part 0x0102030405, 5, 0 => 0x
//@ run-call: part 0x0102, 1, 3 => 0x020000

// The same copy in assembly. Three bytes from offset one of a two-byte slice
// is one byte of the slice and two of the calldata after it.
// CHECK-LABEL: fn @part
// CHECK: calldatacopy
contract Unsafe {
    function part(bytes calldata src, uint256 start, uint256 count) public pure returns (bytes memory out) {
        out = new bytes(count);
        assembly ("memory-safe") {
            calldatacopy(add(out, 0x20), add(src.offset, start), count)
        }
    }
}
