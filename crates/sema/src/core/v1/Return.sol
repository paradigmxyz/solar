// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Ending the current call successfully from any function.
/// @dev Compiler-owned module, imported as `solar:core/v1/Return.sol`. A
/// `return` statement leaves only the function it is in; these end the whole
/// call, the way `Revert.raw` ends it with a failure. Nothing after a call to
/// them runs. `abiEncoded` returns its argument encoded as the single result
/// of a function returning that type, and `raw` returns exactly its bytes,
/// which only a fallback function's output can be. This compiler checks each
/// call against every entry point that can reach it and rejects one reachable
/// from creation, where the returned bytes would become the deployed code.
/// A literal converts to more than one overload's type, so it names the type:
/// `Return.abiEncoded(string("done"))`, `Return.abiEncoded(uint256(1))`.
/// The bodies are memory-safe assembly returns of the encoding; the compiler
/// lowers each call to a return terminator by module identity and encodes in
/// place, since the call ends before memory is read again.
library Return {
    /// @dev Ends the call, returning `s` ABI-encoded as the single result of a
    /// function that returns `string`.
    function abiEncoded(string memory s) internal pure {
        bytes memory data = abi.encode(s);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `b` ABI-encoded as the single result of a
    /// function that returns `bytes`.
    function abiEncoded(bytes memory b) internal pure {
        bytes memory data = abi.encode(b);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `value` as the single result of a function
    /// that returns `uint256`.
    function abiEncoded(uint256 value) internal pure {
        bytes memory data = abi.encode(value);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `value` as the single result of a function
    /// that returns `int256`.
    function abiEncoded(int256 value) internal pure {
        bytes memory data = abi.encode(value);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `value` as the single result of a function
    /// that returns `address`.
    function abiEncoded(address value) internal pure {
        bytes memory data = abi.encode(value);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `value` as the single result of a function
    /// that returns `bool`.
    function abiEncoded(bool value) internal pure {
        bytes memory data = abi.encode(value);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning `value` as the single result of a function
    /// that returns `bytes32`.
    function abiEncoded(bytes32 value) internal pure {
        bytes memory data = abi.encode(value);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }

    /// @dev Ends the call, returning exactly `data`: the output of a fallback
    /// function, which no ABI describes.
    function raw(bytes memory data) internal pure {
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }
}
