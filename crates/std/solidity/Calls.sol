// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice External calls whose output lands in a buffer the caller owns, or
/// ends the current call.
/// @dev Compiler-owned module, imported as `solar:core/Calls.sol`.
/// `target.call(payload)` copies the whole response into fresh memory;
/// `callInto` and its siblings copy at most `output.length` bytes of it into
/// `output`, report how many arrived and how many there were, and leave the
/// rest of the buffer alone; `callBounded` and `staticCallBounded` copy at most
/// `maxCopy` bytes into a new buffer allocated once the size is known. `success`
/// is the EVM's, not the callee's: a reverting callee gives `false` with its
/// revert data as the output.
/// `forward` and `forwardDelegate` pass calldata on and end the current call
/// with the whole response: its return data when the callee succeeds, as the
/// revert when it fails. That output is raw bytes, which only a fallback
/// function returns, and this compiler checks each call against every entry
/// point that can reach it, as it checks `Return.raw`. The bodies are one
/// memory-safe assembly call each; nothing here bounds what the callee may do.
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

    /// @dev Calls `target` with `payload`, `value` wei and `gasLimit` gas, and
    /// copies at most `maxCopy` bytes of the response into a new buffer sized to
    /// what it holds, allocated after the call. `totalSize` is how many bytes
    /// there were.
    function callBounded(
        address target,
        uint256 value,
        uint256 gasLimit,
        bytes memory payload,
        uint256 maxCopy
    ) internal returns (bool success, bytes memory output, uint256 totalSize) {
        assembly ("memory-safe") {
            success := call(gasLimit, target, value, add(payload, 0x20), mload(payload), 0x00, 0x00)
            totalSize := returndatasize()
            let copied := totalSize
            if gt(copied, maxCopy) { copied := maxCopy }
            output := mload(0x40)
            mstore(output, copied)
            returndatacopy(add(output, 0x20), 0x00, copied)
            mstore(0x40, and(add(add(output, 0x3f), copied), not(0x1f)))
        }
    }

    /// @dev Static-calls `target` with `payload` and `gasLimit` gas, and copies
    /// at most `maxCopy` bytes of the response into a new buffer, as
    /// `callBounded` does.
    function staticCallBounded(address target, uint256 gasLimit, bytes memory payload, uint256 maxCopy)
        internal
        view
        returns (bool success, bytes memory output, uint256 totalSize)
    {
        assembly ("memory-safe") {
            success := staticcall(gasLimit, target, add(payload, 0x20), mload(payload), 0x00, 0x00)
            totalSize := returndatasize()
            let copied := totalSize
            if gt(copied, maxCopy) { copied := maxCopy }
            output := mload(0x40)
            mstore(output, copied)
            returndatacopy(add(output, 0x20), 0x00, copied)
            mstore(0x40, and(add(add(output, 0x3f), copied), not(0x1f)))
        }
    }

    /// @dev Calls `target` with `data`, `value` wei and all the gas left, and
    /// ends the current call with the response.
    function forward(address target, uint256 value, bytes calldata data) internal {
        assembly ("memory-safe") {
            let input := mload(0x40)
            calldatacopy(input, data.offset, data.length)
            let success := call(gas(), target, value, input, data.length, 0x00, 0x00)
            returndatacopy(input, 0x00, returndatasize())
            if iszero(success) { revert(input, returndatasize()) }
            return(input, returndatasize())
        }
    }

    /// @dev Delegate-calls `target` with `data` and all the gas left, and ends
    /// the current call with the response. The callee runs with this
    /// contract's storage and balance.
    function forwardDelegate(address target, bytes calldata data) internal {
        assembly ("memory-safe") {
            let input := mload(0x40)
            calldatacopy(input, data.offset, data.length)
            let success := delegatecall(gas(), target, input, data.length, 0x00, 0x00)
            returndatacopy(input, 0x00, returndatasize())
            if iszero(success) { revert(input, returndatasize()) }
            return(input, returndatasize())
        }
    }
}
