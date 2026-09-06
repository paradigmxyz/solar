//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// Integer-to-enum conversions must check the actual variant count rather than
// merely truncating to the enum's uint8 representation. Both qualified and
// unqualified enum callees panic with code 0x21 at their respective bounds, 3 and 2.

library DataTypes {
    enum Mode {
        NONE,
        STABLE,
        VARIABLE
    }
}

contract E {
    // CHECK-LABEL: @module E_runtime
    // CHECK: push 0xb4702ebe
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[LOCAL:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: push 0xbc477c04
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[WRAPPER:bb[0-9]+]]
    // CHECK-NEXT: [[WRAPPER]]:
    // CHECK: push 3{{$}}
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[BODY:bb[0-9]+]], [[PANIC:bb[0-9]+]]
    // CHECK-NEXT: [[PANIC]]:
    // CHECK-NEXT: push 0x4e487b71
    // CHECK-NEXT: push 224
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 33
    // CHECK-NEXT: push 4
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 36
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK-NEXT: [[BODY]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: iszero
    // CHECK: return
    // CHECK: [[REJECT]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    function isNone(uint256 x) external pure returns (bool) {
        return DataTypes.Mode(x) == DataTypes.Mode.NONE;
    }

    enum LocalMode {
        NONE,
        STABLE
    }

    // CHECK: [[LOCAL]]:
    // CHECK: push 2{{$}}
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[BODY]], [[PANIC]]
    function isLocalNone(uint256 x) external pure returns (bool) {
        return LocalMode(x) == LocalMode.NONE;
    }
}
