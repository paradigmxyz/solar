//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

// An explicit enum conversion written through its container, `Container.Enum(x)`
// (the callee is a member access resolving to an enum), is the identity on the
// underlying integer — matching the plain `Enum(x)` (`Ident` callee) path.
// Used by aave-v3-core FlashLoanLogic:
//   `DataTypes.InterestRateMode(params.interestRateModes[i]) == ...NONE`.
// Runtime behavior is verified against solc 0.8.30 separately.

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
    // CHECK-NEXT: iszero
    // CHECK: return
    function isNone(uint256 x) external pure returns (bool) {
        return DataTypes.Mode(x) == DataTypes.Mode.NONE;
    }
}
