//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The dispatcher belongs to the contract definition, as in solc: `compileContract` sets the
// contract's location around `appendFunctionSelector`, which covers the selector load and
// switch, the hoisted callvalue check, and the revert for an unknown selector.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb0: {{.*}}runtime dispatch entry
// CHECK-NEXT: callvalue !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: push [[BB:bb[0-9]+]] !metadata(span=[[C]])
// CHECK-NEXT: jumpi !metadata(span=[[C]])
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[C]])
// CHECK: calldataload !metadata(span=[[C]])
// CHECK-NEXT: push 224 !metadata(span=[[C]])
// CHECK-NEXT: shr !metadata(span=[[C]])
// CHECK: jumpi !metadata(span=[[C]])
// CHECK: push 0 !metadata(span=[[C]])
// CHECK-NEXT: push 0 !metadata(span=[[C]])
// CHECK-NEXT: revert !metadata(span=[[C]])
contract C {
    uint256 x;

    function set(uint256 v) external {
        x = v;
    }

    function get() external view returns (uint256) {
        return x;
    }
}
