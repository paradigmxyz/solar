//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A jump to the next block emits no opcode, but the jump still carries the origin of the MIR
// terminator it implements: none for the jump out of the arm of an `if` statement, which
// implements no source construct, and the conditional expression for a ternary.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK-LABEL: ; function f(
// CHECK: push 0x1172b83c7d517adce
// CHECK: shr !metadata(span=
// CHECK-NEXT: jump bb{{[0-9]+}}{{$}}
// CHECK-LABEL: ; function g(
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

    function g(uint256 x, uint256 y) external pure returns (uint256 r) {
        unchecked {
            r = x & 0xff > 0 ? (((y * 0x1306fe0a31b7152df) >> 64) * 0x1172b83c7d517adce) >> 64 : 0;
            r += 1;
        }
    }
}
