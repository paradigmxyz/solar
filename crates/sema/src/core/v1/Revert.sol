// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Reverting with exact bytes.
/// @dev Compiler-owned module, imported as `solar:core/v1/Revert.sol`.
/// `revert(string(data))` ABI-encodes its argument as an `Error(string)`;
/// `raw` reverts with `data` itself, which is what bubbling another call's
/// revert data or returning a pre-encoded error needs. The body is one
/// memory-safe assembly revert, and the compiler lowers the call to a revert
/// terminator carrying the same range.
library Revert {
    /// @dev Reverts the current call with exactly `data`.
    function raw(bytes memory data) internal pure {
        assembly ("memory-safe") {
            revert(add(data, 0x20), mload(data))
        }
    }
}
