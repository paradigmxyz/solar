//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A library's guard against direct calls and the jump into the function take the routed function's
// declaration, as in solc's per-function `LocationSetter` in `appendFunctionSelector`
// (`ContractCompiler.cpp` L514), while the selector switch around them belongs to the library
// definition.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 224 !metadata(span=[[L:[0-9]+..[0-9]+]])
// CHECK: jumpi !metadata(span=[[L]])
// CHECK: push_immutable 0, 20 !metadata(span=[[WRITE:[0-9]+..[0-9]+]])
// CHECK-NEXT: address !metadata(span=[[WRITE]])
// CHECK-NEXT: eq !metadata(span=[[WRITE]])
// CHECK-NEXT: push {{bb[0-9]+}} !metadata(span=[[WRITE]])
// CHECK-NEXT: jumpi !metadata(span=[[WRITE]])
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[WRITE]])
// CHECK-NEXT: {{bb[0-9]+}}:
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[WRITE]])
library L {
    function read(uint256 x) external pure returns (uint256) {
        return x + 1;
    }

    function write(uint256[] storage a) external {
        a.push(1);
    }
}
