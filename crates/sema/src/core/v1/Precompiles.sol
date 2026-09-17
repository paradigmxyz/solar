// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Calls to the arithmetic precompiles that say whether they worked.
/// @dev Compiler-owned module, imported as `solar:core/v1/Precompiles.sol`.
/// A precompile that rejects its input, or a chain that lacks it, leaves the
/// caller's output memory untouched, and code that does not look reads
/// whatever was there. Every wrapper here reports `success` separately from
/// the result, requires the response to have exactly the size the operation
/// defines, and returns zeroed results when it does not succeed. What a
/// successful result means cryptographically is the caller's concern. Each
/// function names the fork it needs; the curve and `modexp` calls exist from
/// Byzantium on. Every body is ordinary Solidity.
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

    /// @dev The BLAKE2b compression function `F` (EIP-152, from Istanbul):
    /// `rounds` rounds over the state `h`, the message block `m` and the offset
    /// counter `t`, all little-endian words as the hash defines them. `result`
    /// is the new state, and zero when the call does not succeed.
    function blake2f(
        uint32 rounds,
        bytes32[2] memory h,
        bytes32[4] memory m,
        bytes8[2] memory t,
        bool finalBlock
    ) internal view returns (bool success, bytes32[2] memory result) {
        bytes memory output;
        (success, output) = address(9).staticcall(
            abi.encodePacked(rounds, h[0], h[1], m[0], m[1], m[2], m[3], t[0], t[1], finalBlock)
        );
        if (!success || output.length != 64) return (false, result);
        (result[0], result[1]) = abi.decode(output, (bytes32, bytes32));
    }

    /// @dev Whether `(r, s)` is a P-256 signature of `hash` under the public
    /// key `(x, y)` (EIP-7951, from Osaka). The precompile answers with one
    /// word for a valid signature and with nothing otherwise, so an invalid
    /// signature, a malformed key, and a chain without the precompile are all
    /// `false`: nothing here can tell them apart.
    function p256Verify(bytes32 hash, uint256 r, uint256 s, uint256 x, uint256 y)
        internal
        view
        returns (bool)
    {
        (bool success, bytes memory output) = address(0x100).staticcall(abi.encode(hash, r, s, x, y));
        return success && output.length == 32 && abi.decode(output, (uint256)) == 1;
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
