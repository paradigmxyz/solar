//@ codegen-matrix: standard ir
//@ compile-flags: -Zevm-ir-pipeline=block-dedup
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: depth 3 => 3

// Machine lowering marks these return labels private, but the bounded block pass
// must still decline a module containing its computed call/return convention.
// CHECK-LABEL: @module PrivateCalls_runtime
// CHECK: push {{bb[0-9]+}}
// CHECK: jump{{$}}
contract PrivateCalls {
    function depth(uint256 n) external pure returns (uint256) {
        return recurse(n);
    }

    function recurse(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return recurse(n - 1) + 1;
    }
}
