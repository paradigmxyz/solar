//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:

// A jump to the next block emits no opcode, but the jump still carries the origin of the MIR
// terminator it implements: here the `if` statement.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 0x1172b83c7d517adce
// CHECK: shr !metadata(span=
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=
contract C {
    function f(uint256 x, uint256 y) external pure returns (uint256 r) {
        unchecked {
            if (x & 0xff > 0) {
                r = (y * 0x1306fe0a31b7152df) >> 64;
                r = (r * 0x1172b83c7d517adce) >> 64;
            }
            r += 1;
        }
    }
}
