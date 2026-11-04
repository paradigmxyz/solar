//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The code that starts the free memory pointer at an external entry belongs to no function. It maps
// to the contract definition, as solc maps the code `compileContract` generates.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=g@
// CHECK-NEXT: push 128 !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: push 64 !metadata(span=[[C]])
// CHECK-NEXT: mstore !metadata(span=[[C]])
contract C {
    function f(bytes memory a) public pure returns (uint256) {
        return a.length;
    }

    function g(bytes memory b) public pure returns (uint256) {
        return b.length + 1;
    }
}
