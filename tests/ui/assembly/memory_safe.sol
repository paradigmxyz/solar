//@ compile-flags: -Zdump=hir
//@ filecheck:
// Test memory-safe assembly flag

contract MemorySafe {
    function memorySafe() public pure returns (uint256 result) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, 42)
            mstore(0x40, add(ptr, 32))
            result := mload(ptr)
        }
    }

    function withDialect() public pure returns (uint256 result) {
        assembly "evmasm" {
            result := 42
        }
    }

    function dialectAndFlag() public pure returns (uint256 result) {
        assembly "evmasm" ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, 100)
            mstore(0x40, add(ptr, 32))
            result := mload(ptr)
        }
    }

    // CHECK-LABEL: function legacyExact(
    // CHECK: assembly ("memory-safe") {
    function legacyExact() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyMultiple(
    // CHECK: assembly ("memory-safe") {
    function legacyMultiple() public pure returns (uint256 result) {
        /// @solidity ignored	memory-safe-assembly other
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidShortMarker(
    // CHECK: assembly {
    function invalidShortMarker() public pure returns (uint256 result) {
        /// @solidity memory-safe
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidSuffix(
    // CHECK: assembly {
    function invalidSuffix() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly-extra
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidPrefix(
    // CHECK: assembly {
    function invalidPrefix() public pure returns (uint256 result) {
        /// @solidity prefix-memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidUnicodeSeparator(
    // CHECK: assembly {
    function invalidUnicodeSeparator() public pure returns (uint256 result) {
        /// @solidity ignored memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidVerticalTabBoundary(
    // CHECK: assembly {
    function invalidVerticalTabBoundary() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidFormFeedTagBoundary(
    // CHECK: assembly {
    function invalidFormFeedTagBoundary() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyBlockContinuation(
    // CHECK: assembly ("memory-safe") {
    function legacyBlockContinuation() public pure returns (uint256 result) {
        /** @solidity
         *memory-safe-assembly
         */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidDoubleStarContinuation(
    // CHECK: assembly {
    function invalidDoubleStarContinuation() public pure returns (uint256 result) {
        /** @solidity
         **memory-safe-assembly
         */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidLineStarContinuation(
    // CHECK: assembly {
    function invalidLineStarContinuation() public pure returns (uint256 result) {
        /// @solidity
        /// *memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyBlockCarriageReturn(
    // CHECK: assembly ("memory-safe") {
    function legacyBlockCarriageReturn() public pure returns (uint256 result) {
        /** @solidity *memory-safe-assembly */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidJoinedBlockSuffix(
    // CHECK: assembly {
    function invalidJoinedBlockSuffix() public pure returns (uint256 result) {
        /** @solidity memory-safe-assembly
         **suffix
         */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacySeparatedBlockSuffix(
    // CHECK: assembly ("memory-safe") {
    function legacySeparatedBlockSuffix() public pure returns (uint256 result) {
        /** @solidity memory-safe-assembly
         *suffix
         */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidJoinedTag(
    // CHECK: assembly {
    function invalidJoinedTag() public pure returns (uint256 result) {
        /** @notice text
         **@solidity memory-safe-assembly */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidJoinedFollowingTag(
    // CHECK: assembly {
    function invalidJoinedFollowingTag() public pure returns (uint256 result) {
        /** @solidity memory-safe-assembly
         **@notice suffix */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyLineContinuation(
    // CHECK: assembly ("memory-safe") {
    function legacyLineContinuation() public pure returns (uint256 result) {
        /// @solidity
        /// memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidConsumedFollowingTag(
    // CHECK: assembly {
    function invalidConsumedFollowingTag() public pure returns (uint256 result) {
        /** @notice
         *@solidity memory-safe-assembly */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidOverwrittenBlock(
    // CHECK: assembly {
    function invalidOverwrittenBlock() public pure returns (uint256 result) {
        /** @solidity memory-safe-assembly */
        /** @notice replacement */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyMultipleBlockTags(
    // CHECK: assembly ("memory-safe") {
    function legacyMultipleBlockTags() public pure returns (uint256 result) {
        /** @notice text
         *@solidity memory-safe-assembly */
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidSkippedEmptyLine(
    // CHECK: assembly {
    function invalidSkippedEmptyLine() public pure returns (uint256 result) {
        /// @notice
        ///
        /// @solidity memory-safe-assembly
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidJoinedClosingStar(
    // CHECK: assembly {
    function invalidJoinedClosingStar() public pure returns (uint256 result) {
        /** @solidity memory-safe-assembly
        **/
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidBlankLineGroup(
    // CHECK: assembly {
    function invalidBlankLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly

        /// @notice continuation
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidIndentedBlankLineGroup(
    // CHECK: assembly {
    function invalidIndentedBlankLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly
 	
        /// @notice continuation
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidCrLfBlankLineGroup(
    // CHECK: assembly {
    function invalidCrLfBlankLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly

        /// @notice continuation
        assembly { result := 42 }
    }

    // CHECK-LABEL: function invalidCrBlankLineGroup(
    // CHECK: assembly {
    function invalidCrBlankLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly        /// @notice continuation
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyCrLfLineGroup(
    // CHECK: assembly ("memory-safe") {
    function legacyCrLfLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly
        /// @notice continuation
        assembly { result := 42 }
    }

    // CHECK-LABEL: function legacyCrLineGroup(
    // CHECK: assembly ("memory-safe") {
    function legacyCrLineGroup() public pure returns (uint256 result) {
        /// @solidity memory-safe-assembly        /// @notice continuation
        assembly { result := 42 }
    }

}
