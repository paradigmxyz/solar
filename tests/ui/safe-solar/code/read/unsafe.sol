//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: part 0x63deadbeef6000526004601cf3, 0, 4 => 0xdeadbeef
//@ run-call: part 0x63deadbeef6000526004601cf3, 1, 2 => 0xadbe
//@ run-call: part 0x63deadbeef6000526004601cf3, 2, 4 => 0xbeef0000

// The same copy in assembly. Asked for four bytes at offset two of four
// bytes of code it does not fail: `extcodecopy` pads with zeros, and the
// caller gets two bytes of code and two bytes of nothing.
// CHECK-LABEL: fn @part
// CHECK: extcodecopy
contract Unsafe {
    function part(bytes memory initcode, uint256 start, uint256 count) public returns (bytes memory out) {
        address target;
        assembly ("memory-safe") {
            target := create(0, add(initcode, 0x20), mload(initcode))
        }
        out = new bytes(count);
        assembly ("memory-safe") {
            extcodecopy(target, add(out, 0x20), start, count)
        }
    }
}
