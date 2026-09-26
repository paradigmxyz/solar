// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Ending the current call successfully from any function.
/// @dev Compiler-owned module, imported as `solar:core/v1/Return.sol`. A
/// `return` statement leaves only the function it is in; these end the whole
/// call, the way `Revert.raw` ends it with a failure. Nothing after a call to
/// them runs. The body is one memory-safe assembly return of the encoding;
/// the compiler lowers the call to a return terminator by module identity and
/// encodes in place, since the call ends before memory is read again.
library Return {
    /// @dev Ends the call, returning `s` ABI-encoded as the single result of a
    /// function that returns `string`.
    function abiEncoded(string memory s) internal pure {
        bytes memory data = abi.encode(s);
        assembly ("memory-safe") {
            return(add(data, 0x20), mload(data))
        }
    }
}
