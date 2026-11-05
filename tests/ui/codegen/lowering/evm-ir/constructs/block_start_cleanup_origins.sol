//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The pop that cleans the stack at the start of the block after the loop belongs to no instruction.
// It takes the origin of the instruction it precedes, the `return`.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: pop !metadata(span=[[R:[0-9]+..[0-9]+]])
// CHECK-NEXT: push 0 !metadata(span=[[R]])
contract C {
    function f(uint256 a) external pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < a; i++) {
            acc += i + 1;
        }
        return acc;
    }
}
