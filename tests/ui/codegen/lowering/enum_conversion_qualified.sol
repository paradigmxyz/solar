//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

// Integer-to-enum conversions must check the actual variant count rather than
// merely truncating to the enum's uint8 representation. Both qualified and
// unqualified enum callees panic with code 0x21 when `x >= 3`.

library DataTypes {
    enum Mode {
        NONE,
        STABLE,
        VARIABLE
    }
}

contract E {
    // CHECK: push 0xbc477c04
    // CHECK: calldataload
    // CHECK-NEXT: lt
    // CHECK-NEXT: iszero
    // CHECK: return
    function isNone(uint256 x) external pure returns (bool) {
        return DataTypes.Mode(x) == DataTypes.Mode.NONE;
    }

    enum LocalMode {
        NONE,
        STABLE
    }

    function isLocalNone(uint256 x) external pure returns (bool) {
        return LocalMode(x) == LocalMode.NONE;
    }
}
