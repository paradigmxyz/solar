//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The pop that cleans the stack at the start of the block after the loop ends the loop counter. It
// takes the origin of the block's label, the `for` it leaves, and not the origin of the `return`
// that follows.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb{{[0-9]+}} !metadata(span=[[FOR:[0-9]+\.\.[0-9]+]]):{{[[:space:]]+}}pop !metadata(span=[[FOR]])
// CHECK-NEXT: push 0 !metadata(span={{[0-9]+\.\.[0-9]+}})
contract C {
    function f(uint256 a) external pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < a; i++) {
            acc += i + 1;
        }
        return acc;
    }
}
