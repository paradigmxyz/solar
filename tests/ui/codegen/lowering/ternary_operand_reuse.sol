//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:

contract TernaryOperandReuse {
    // Keep the first caller result for the returned address. The late physical
    // rewrite rereads the cheap environment value for the ternary operands;
    // it does not move the retained return value or insert a dead pop.
    // CHECK-LABEL: @module TernaryOperandReuse_runtime
    // CHECK: push 0x718ec94
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ADD_CHECK:bb[0-9]+]], [[MUL_SELECTOR:bb[0-9]+]]
    // CHECK-NEXT: [[MUL_SELECTOR]]:
    // CHECK-NEXT: push 0xfd9e0caf
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[MUL_CHECK:bb[0-9]+]]
    // CHECK-NEXT: [[MUL_CHECK]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[MUL_BODY:bb[0-9]+]]
    // CHECK-NEXT: [[MUL_BODY]]:
    // CHECK-NEXT: caller
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: caller
    // CHECK-NEXT: caller
    // CHECK-NEXT: mulmod
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 64
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[ADD_CHECK]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[ADD_BODY:bb[0-9]+]]
    // CHECK-NEXT: [[ADD_BODY]]:
    // CHECK-NEXT: caller
    // CHECK-NOT: pop
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: caller
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: addmod
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 64
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    function addCaller(uint256 x, uint256 y) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := addmod(x, y, sender)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }

    // MULMOD receives two fresh caller words; the first caller word remains
    // available for the address returned alongside the modular result.
    function mulRepeated(uint256 modulus) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := mulmod(sender, sender, modulus)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }
}
