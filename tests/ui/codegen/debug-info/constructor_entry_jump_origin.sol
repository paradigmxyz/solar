//@ compile-flags: -Onone --emit=srcmap -Zevm-ir-pipeline=none -Zdump=evm-ir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The jump over the constructor's helpers is part of entering the constructor. It maps to the
// constructor's declaration, as the argument copy before it does. Without a declared constructor,
// the synthesized one is declared by the contract, and the jump maps to the contract.

// CHECK-LABEL: @module C_deployment
// CHECK: mstore !metadata(span=[[#CTOR_START:]]..[[#CTOR_END:]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[#CTOR_START]]..[[#CTOR_END]])
// CHECK-NEXT: bb{{[0-9]+}} [invoke=helper@
contract C {
    uint256 public x;

    function helper(uint256 a) internal pure returns (uint256) {
        return a + 1;
    }

    constructor(uint256 a) {
        x = helper(a);
    }
}

// CHECK-LABEL: @module D_deployment
// CHECK: jump bb{{[0-9]+}} !metadata(span=[[#D_START:]]..[[#D_END:]])
// CHECK-NEXT: bb{{[0-9]+}} [invoke=helper@
// CHECK: bb{{[0-9]+}} !metadata(span=[[#D_START]]..[[#D_END]]):{{.*}}creation entry
contract D {
    function helper() internal pure returns (uint256) {
        return 1;
    }

    uint256 public x = helper();
}
