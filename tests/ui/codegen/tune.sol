//@ revisions: none interpreter jit
//@[none] compile-flags: -O gas -Ztune=none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefixes=CHECK,NONE
//@[interpreter] compile-flags: -O gas -Ztune=interpreter -Zdump=evm-ir-runtime
//@[interpreter] filecheck: --check-prefixes=CHECK,INTERPRETER
//@[jit] compile-flags: -O gas -Ztune=jit -Zdump=evm-ir-runtime
//@[jit] filecheck: --check-prefixes=CHECK,JIT
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: mask 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0 => 0x0000000000000000000000009abcdef0123456789abcdef0123456789abcdef0
//@ run-call: invert 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: pick 0 => 10
//@ run-call: pick 3 => 40
//@ run-call: pick 4 => 0

contract AddressMask {
    // The address mask costs three more executed instructions as a recipe than as a literal,
    // which an interpreter dispatches and a JIT folds. `not(0)` saves 31 bytes for one
    // instruction, so every engine keeps its recipe.
    // CHECK-LABEL: @module AddressMask_runtime
    // NONE: push 0
    // NONE-NEXT: not
    // NONE-NEXT: push 96
    // NONE-NEXT: shr
    // NONE-NEXT: and
    // INTERPRETER: push 0xffffffffffffffffffffffffffffffffffffffff
    // INTERPRETER-NEXT: and
    // JIT: push 0
    // JIT-NEXT: not
    // JIT-NEXT: push 96
    // JIT-NEXT: shr
    // JIT-NEXT: and
    // CHECK: mstore
    // CHECK-NEXT: push 0
    // CHECK-NEXT: not
    // CHECK-NEXT: push 32
    fallback() external {
        assembly {
            mstore(0, and(calldataload(4), 0xffffffffffffffffffffffffffffffffffffffff))
            mstore(32, not(0))
            return(0, 64)
        }
    }
}

contract DenseSwitch {
    // Four adjacent keys use an indexed jump, which evm2's JIT cannot resolve, so the JIT
    // tests each key.
    // CHECK-LABEL: @module DenseSwitch_runtime
    // NONE: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
    // INTERPRETER: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
    // JIT-NOT: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { return(0, 1) }
            case 1 { return(0, 2) }
            case 2 { return(0, 3) }
            case 3 { return(0, 4) }
            default { revert(0, 0) }
        }
    }
}

contract Runtime {
    function mask(uint x) external pure returns (uint r) {
        assembly {
            r := and(x, 0xffffffffffffffffffffffffffffffffffffffff)
        }
    }

    function invert(uint x) external pure returns (uint r) {
        assembly {
            r := not(x)
        }
    }

    function pick(uint x) external pure returns (uint r) {
        assembly {
            switch x
            case 0 { r := 10 }
            case 1 { r := 20 }
            case 2 { r := 30 }
            case 3 { r := 40 }
            default { r := 0 }
        }
    }
}
