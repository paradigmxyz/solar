//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A trampoline runs only on the edge of one branch, so its `JUMPDEST` takes the origin of its body,
// the origin of that branch, like the return tag of a call.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb{{[0-9]+}} !metadata(span=[[IF:[0-9]+\.\.[0-9]+]]):{{[[:space:]]+}}swap 2 !metadata(span=[[IF]])
// CHECK-NEXT: pop !metadata(span=[[IF]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[IF]])
contract C {
    function choose(uint256 a, uint256 b, uint256 c, uint256 n)
        public pure returns (uint256, uint256, uint256)
    {
        unchecked {
            while (n != 0) {
                if (a & 1 != 0) {
                    b = a;
                } else {
                    a += 1;
                }
                --n;
            }
        }
        return (a, b, c);
    }
}
