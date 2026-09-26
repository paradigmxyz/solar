//@ revisions: normal none gas size mir
//@ compile-flags: -Zdump=mir,evm-ir-runtime
//@ run-call: used 41 => 42
//@[normal] compile-flags: -Ogas
//@[normal] filecheck: --check-prefix=NORMAL
//@[none] compile-flags: -Onone -Zcodegen-all-functions
//@[none] filecheck: --check-prefix=ALL
//@[gas] compile-flags: -Ogas -Zcodegen-all-functions
//@[gas] filecheck: --check-prefix=ALL
//@[size] compile-flags: -Osize -Zcodegen-all-functions
//@[size] filecheck: --check-prefix=ALL
//@[mir] compile-flags: -Ogas -Zcodegen-all-functions
//@[mir] filecheck: --check-prefix=MIR

library UnusedFunctions {
    // NORMAL-LABEL: @module UnusedFunctions_runtime
    // NORMAL-NOT: sstore
    // ALL-LABEL: @module UnusedFunctions_runtime
    // ALL: sstore
    function unused(uint256 value) internal {
        assembly { sstore(0, value) }
    }
}

contract UsedFunctions {
    // NORMAL-LABEL: @module UsedFunctions_runtime
    // NORMAL-NOT: tstore
    // ALL-LABEL: @module UsedFunctions_runtime
    // ALL: tstore
    function unused(uint256 value) private {
        assembly { tstore(0, value) }
    }

    function used(uint256 value) external pure returns (uint256) {
        return helper(value);
    }

    // MIR-LABEL: @module UsedFunctions
    // MIR: fn @helper(
    function helper(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }
}
