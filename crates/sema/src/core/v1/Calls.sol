// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice External calls whose output lands in a buffer the caller owns.
/// @dev Compiler-owned module, imported as `solar:core/v1/Calls.sol`.
/// `target.call(payload)` copies the whole response into fresh memory;
/// these copy at most `output.length` bytes of it into `output`, report how
/// many arrived and how many there were, and leave the rest of the buffer
/// alone. `success` is the EVM's, not the callee's: a reverting callee gives
/// `false` with its revert data in `output`. The bodies are one memory-safe
/// assembly call each; nothing here bounds what the callee may do.
library Calls {
    /// @dev Calls `target` with `payload`, `value` wei and `gasLimit` gas.
    function callInto(
        address target,
        uint256 value,
        uint256 gasLimit,
        bytes memory payload,
        bytes memory output
    ) internal returns (bool success, uint256 copied, uint256 totalSize) {
        assembly ("memory-safe") {
            success :=
                call(
                    gasLimit,
                    target,
                    value,
                    add(payload, 0x20),
                    mload(payload),
                    add(output, 0x20),
                    mload(output)
                )
            totalSize := returndatasize()
            copied := totalSize
            if gt(copied, mload(output)) { copied := mload(output) }
        }
    }

    /// @dev Static-calls `target` with `payload` and `gasLimit` gas.
    function staticCallInto(address target, uint256 gasLimit, bytes memory payload, bytes memory output)
        internal
        view
        returns (bool success, uint256 copied, uint256 totalSize)
    {
        assembly ("memory-safe") {
            success :=
                staticcall(
                    gasLimit, target, add(payload, 0x20), mload(payload), add(output, 0x20), mload(output)
                )
            totalSize := returndatasize()
            copied := totalSize
            if gt(copied, mload(output)) { copied := mload(output) }
        }
    }

    /// @dev Delegate-calls `target` with `payload` and `gasLimit` gas. The
    /// callee runs with this contract's storage and balance.
    function delegateCallInto(address target, uint256 gasLimit, bytes memory payload, bytes memory output)
        internal
        returns (bool success, uint256 copied, uint256 totalSize)
    {
        assembly ("memory-safe") {
            success :=
                delegatecall(
                    gasLimit, target, add(payload, 0x20), mload(payload), add(output, 0x20), mload(output)
                )
            totalSize := returndatasize()
            copied := totalSize
            if gt(copied, mload(output)) { copied := mload(output) }
        }
    }
}
