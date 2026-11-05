//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:

// The code that starts the free memory pointer at an external entry belongs to no function. It maps
// to the contract definition, as solc maps the code `compileContract` generates. `g` is the last function, so the contract's
// range ends two bytes after the function's, at the closing brace.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [entry, invoke=g@{{[0-9]+}}..[[#G_END:]]]
// CHECK-NEXT: push 128 !metadata(span={{[0-9]+}}..[[#G_END + 2]])
// CHECK-NEXT: push 64 !metadata(span={{[0-9]+}}..[[#G_END + 2]])
// CHECK-NEXT: mstore !metadata(span={{[0-9]+}}..[[#G_END + 2]])
contract C {
    function f(bytes memory a) public pure returns (uint256) {
        return a.length;
    }

    function g(bytes memory b) public pure returns (uint256) {
        return b.length + 1;
    }
}
