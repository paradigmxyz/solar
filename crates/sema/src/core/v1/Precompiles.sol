// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Calls to the arithmetic precompiles that say whether they worked.
/// @dev Compiler-owned module, imported as `solar:core/v1/Precompiles.sol`.
/// A precompile that rejects its input, or a chain that lacks it, leaves the
/// caller's output memory untouched, and code that does not look reads
/// whatever was there. Every wrapper here reports `success` separately from
/// the result, requires the response to have exactly the size the operation
/// defines, and returns zeroed results when it does not succeed. What a
/// successful result means cryptographically is the caller's concern. All of
/// these exist from Byzantium on. Every body is ordinary Solidity.
library Precompiles {
    /// @dev `base ** exponent % modulus` over big-endian integers of any
    /// length. `result` has the modulus's length.
    function modexp(bytes memory base, bytes memory exponent, bytes memory modulus)
        internal
        view
        returns (bool success, bytes memory result)
    {
        (success, result) = address(5).staticcall(
            abi.encodePacked(base.length, exponent.length, modulus.length, base, exponent, modulus)
        );
        if (!success || result.length != modulus.length) return (false, "");
    }

    /// @dev The sum of two points of the alt_bn128 curve. Fails when either is
    /// not on the curve.
    function ecAdd(uint256 x1, uint256 y1, uint256 x2, uint256 y2)
        internal
        view
        returns (bool success, uint256 x, uint256 y)
    {
        bytes memory output;
        (success, output) = address(6).staticcall(abi.encode(x1, y1, x2, y2));
        if (!success || output.length != 64) return (false, 0, 0);
        (x, y) = abi.decode(output, (uint256, uint256));
    }

    /// @dev A point of the alt_bn128 curve multiplied by `scalar`. Fails when
    /// the point is not on the curve.
    function ecMul(uint256 x1, uint256 y1, uint256 scalar)
        internal
        view
        returns (bool success, uint256 x, uint256 y)
    {
        bytes memory output;
        (success, output) = address(7).staticcall(abi.encode(x1, y1, scalar));
        if (!success || output.length != 64) return (false, 0, 0);
        (x, y) = abi.decode(output, (uint256, uint256));
    }

    /// @dev The alt_bn128 pairing check over `input`, a sequence of 192-byte
    /// pairs. Fails when the length is not a whole number of pairs or a point
    /// is invalid; `result` is whether the product of pairings is one.
    function ecPairing(bytes memory input) internal view returns (bool success, bool result) {
        if (input.length % 192 != 0) return (false, false);
        bytes memory output;
        (success, output) = address(8).staticcall(input);
        if (!success || output.length != 32) return (false, false);
        result = abi.decode(output, (uint256)) == 1;
    }
}
