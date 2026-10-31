//@ compile-flags: -O none -Zdump=disasm-runtime
//@ filecheck:

// A function tagged `@custom:solar-optimize size` encodes its constants as a size build does,
// from its first instruction on, though the build is `-O none`: it builds `1e18` with `EXP` where
// the untagged function pushes it whole.
contract Encoding {
    function plain(uint256 x) external pure returns (uint256) {
        return x ^ 1000000000000000000;
    }

    /// @custom:solar-optimize size
    function small(uint256 x) external pure returns (uint256) {
        return x | 1000000000000000000;
    }
}

// CHECK: PUSH8 0x0de0b6b3a7640000
// CHECK-NOT: PUSH8 0x0de0b6b3a7640000
// CHECK: EXP
