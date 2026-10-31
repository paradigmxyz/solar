//@ compile-flags: -O none -Zdump=disasm-runtime
//@ filecheck:

// A function tagged `@custom:solar-optimize size` encodes its constants as a size build does,
// from its first instruction on, though the build is `-O none`: it holds the repeated word by
// division where the untagged function pushes it whole.
contract Encoding {
    function plain(uint256 x) external pure returns (uint256) {
        return x ^ 0x0101010101010101010101010101010101010101010101010101010101010101;
    }

    /// @custom:solar-optimize size
    function small(uint256 x) external pure returns (uint256) {
        return x | 0x0101010101010101010101010101010101010101010101010101010101010101;
    }
}

// CHECK: PUSH32 0x0101010101010101010101010101010101010101010101010101010101010101
// CHECK-NOT: PUSH32 0x0101010101010101010101010101010101010101010101010101010101010101
// CHECK: DIV
