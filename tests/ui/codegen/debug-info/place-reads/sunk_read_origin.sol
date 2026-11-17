//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A read that `place-reads` copies into the block that dominates its uses is a new instruction in a
// block that did not hold it, so it has no origin, as LLVM's `dropLocation` for code moved to
// another block. It does not send a debugger back to the function header, the origin of the
// parameter read, in the middle of the body. The read that stays where it was keeps that origin.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 4 !metadata(span=[[HEADER:[0-9]+..[0-9]+]])
// CHECK-NEXT: calldataload !metadata(span=[[HEADER]])
// CHECK: push 68{{$}}
// CHECK-NEXT: calldataload{{$}}
contract C {
    function complexExpr(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        return (a + b) * c - (a / (b + 1));
    }
}
