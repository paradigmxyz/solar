//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The exit of the first loop is an empty block that jumps into the second loop, so edges into it go
// straight to the second loop. Its label marked where control entered, the first `for`, so the
// label of the second loop's block takes that origin as well as its own.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=fill@
// CHECK: bb{{[0-9]+}} [loop] !metadata(span=[[FIRST:[0-9]+\.\.[0-9]+]]):
// CHECK: bb{{[0-9]+}} [loop] !metadata(spans=[{{[0-9]+\.\.[0-9]+}}, [[FIRST]]]):
contract C {
    function fill() public pure returns (uint256[] memory a) {
        a = new uint256[](6);
        uint256 i;
        for (; i < 2; i++) {
            a[i] = 8;
        }
        for (; i < 5; i++) {
            a[i] = 9;
        }
    }
}
