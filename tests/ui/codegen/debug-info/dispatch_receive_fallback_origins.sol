//@ compile-flags: -Ogas --revert-strings debug --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// With `receive`, `fallback` and revert strings, the empty-calldata check, the selector switch and
// the `Error(string)` revert are all part of the dispatcher, which belongs to the contract
// definition as in solc.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb0: {{.*}}runtime dispatch entry
// CHECK-NEXT: calldatasize !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: push {{bb[0-9]+}} !metadata(span=[[C]])
// CHECK-NEXT: jumpi !metadata(span=[[C]])
// CHECK: shr !metadata(span=[[C]])
// CHECK: mstore !metadata(span=[[C]])
// CHECK: revert !metadata(span=[[C]])
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
