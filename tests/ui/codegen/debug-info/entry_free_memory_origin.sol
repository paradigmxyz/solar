//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The code that starts the free memory pointer at an external entry stores that entry's own heap
// floor, after dispatch. It maps to the entry's declaration, not to the contract: solc runs one
// contract-wide preamble before dispatch, and ours runs per entry.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=g@[[G:[0-9]+..[0-9]+]]]
// CHECK-NEXT: push 128 !metadata(span=[[G]])
// CHECK-NEXT: push 64 !metadata(span=[[G]])
// CHECK-NEXT: mstore !metadata(span=[[G]])
contract C {
    function f(bytes memory a) public pure returns (uint256) {
        return a.length;
    }

    function g(bytes memory b) public pure returns (uint256) {
        return b.length + 1;
    }
}
