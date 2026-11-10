//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The `receive` and `fallback` blocks are routes that run only for their function, as a selector
// route does, so they and a non-payable fallback's `callvalue` check map to the function's
// declaration. solc maps that check to the contract (`ContractCompiler.cpp` L485), which also
// covers the selector routes and the revert that this block does not run for.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb0: {{.*}}runtime dispatch entry
// CHECK: bb{{[0-9]+}} !metadata(span=[[RECEIVE:[0-9]+..[0-9]+]]):
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[RECEIVE]])
// CHECK: eq
// CHECK: jump {{bb[0-9]+}}
// CHECK-NEXT: {{bb[0-9]+}} !metadata(span={{[0-9]+..[0-9]+}}):
// CHECK-NEXT: jump
// CHECK-NEXT: {{bb[0-9]+}} !metadata(span=[[FALLBACK:[0-9]+..[0-9]+]]):
// CHECK-NEXT: callvalue !metadata(span=[[FALLBACK]])
// CHECK-NEXT: push {{bb[0-9]+}} !metadata(span=[[FALLBACK]])
// CHECK-NEXT: jumpi !metadata(span=[[FALLBACK]])
// CHECK: bb{{[0-9]+}} [entry, invoke=receive@[[RECEIVE]]]
// CHECK: bb{{[0-9]+}} [entry, invoke=fallback@[[FALLBACK]]]
contract C {
    uint256 x;

    receive() external payable {
        x += 1;
    }

    fallback() external {
        x += 2;
    }

    function set(uint256 v) external payable {
        x = v;
    }
}
