//@ compile-flags: --emit=bin-runtime --evm-version osaka -Zemit-lean-stages=../../target/ui-lean-stages
// Reverts that share their tail jump on labels their predecessors pushed, which the semantics
// runs as a conditional jump to the label on the stack, so their stages export.

contract LeanStagesSharedTail {
    function narrow8(uint256 value) external pure returns (uint8) {
        require(value <= type(uint8).max, "SafeCast: value doesn't fit in 8 bits");
        return uint8(value);
    }

    function narrow32(uint256 value) external pure returns (uint32) {
        require(value <= type(uint32).max, "SafeCast: value doesn't fit in 32 bits");
        return uint32(value);
    }

    function narrow128(uint256 value) external pure returns (uint128) {
        require(value <= type(uint128).max, "SafeCast: value doesn't fit in 128 bits");
        return uint128(value);
    }
}
