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
///
/// The BLS12-381 calls (EIP-2537, from Prague) take and return points in the
/// precompiles' encoding: a base field element is 64 big-endian bytes whose
/// top 16 are zero, a point of G1 is its two coordinates (128 bytes), and a
/// point of G2 is its two coordinates over the extension field, each as its
/// two base field elements (256 bytes). An input of the wrong length is
/// rejected here, before any call.
library Precompiles {
    /// @dev Bytes of a BLS12-381 base field element.
    uint256 private constant BLS12_FP = 64;

    /// @dev Bytes of a point of G1.
    uint256 private constant BLS12_G1 = 128;

    /// @dev Bytes of a point of G2.
    uint256 private constant BLS12_G2 = 256;

    /// @dev Bytes of a scalar.
    uint256 private constant BLS12_SCALAR = 32;

    /// @dev What the point evaluation precompile answers when a proof holds:
    /// the field elements per blob and the BLS12-381 scalar field modulus.
    uint256 private constant BLOB_FIELD_ELEMENTS = 4096;
    uint256 private constant BLS12_MODULUS =
        0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001;

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

    /// @dev The sum of two points of BLS12-381's G1. Fails when either is not
    /// a point of the curve.
    function bls12G1Add(bytes memory a, bytes memory b)
        internal
        view
        returns (bool success, bytes memory result)
    {
        if (a.length != BLS12_G1 || b.length != BLS12_G1) return (false, "");
        return _bls12(address(0x0b), bytes.concat(a, b), BLS12_G1);
    }

    /// @dev The sum of the points of G1 in `pairs`, each multiplied by its
    /// scalar: a sequence of 160-byte pairs of a point and a 32-byte scalar.
    /// Fails when there is no pair or a point is not in the subgroup.
    function bls12G1Msm(bytes memory pairs)
        internal
        view
        returns (bool success, bytes memory result)
    {
        uint256 pair = BLS12_G1 + BLS12_SCALAR;
        if (pairs.length == 0 || pairs.length % pair != 0) return (false, "");
        return _bls12(address(0x0c), pairs, BLS12_G1);
    }

    /// @dev The sum of two points of BLS12-381's G2. Fails when either is not
    /// a point of the curve.
    function bls12G2Add(bytes memory a, bytes memory b)
        internal
        view
        returns (bool success, bytes memory result)
    {
        if (a.length != BLS12_G2 || b.length != BLS12_G2) return (false, "");
        return _bls12(address(0x0d), bytes.concat(a, b), BLS12_G2);
    }

    /// @dev The sum of the points of G2 in `pairs`, each multiplied by its
    /// scalar: a sequence of 288-byte pairs of a point and a 32-byte scalar.
    /// Fails when there is no pair or a point is not in the subgroup.
    function bls12G2Msm(bytes memory pairs)
        internal
        view
        returns (bool success, bytes memory result)
    {
        uint256 pair = BLS12_G2 + BLS12_SCALAR;
        if (pairs.length == 0 || pairs.length % pair != 0) return (false, "");
        return _bls12(address(0x0e), pairs, BLS12_G2);
    }

    /// @dev The BLS12-381 pairing check over `pairs`, a sequence of 384-byte
    /// pairs of a point of G1 and a point of G2. Fails when there is no pair or
    /// a point is not in its subgroup; `result` is whether the product of the
    /// pairings is one.
    function bls12Pairing(bytes memory pairs) internal view returns (bool success, bool result) {
        uint256 pair = BLS12_G1 + BLS12_G2;
        if (pairs.length == 0 || pairs.length % pair != 0) return (false, false);
        bytes memory output;
        (success, output) = address(0x0f).staticcall(pairs);
        if (!success || output.length != 32) return (false, false);
        uint256 answer = abi.decode(output, (uint256));
        if (answer > 1) return (false, false);
        result = answer == 1;
    }

    /// @dev The point of G1 a base field element maps to. Fails when the
    /// element is not below the field modulus.
    function bls12MapFpToG1(bytes memory element)
        internal
        view
        returns (bool success, bytes memory result)
    {
        if (element.length != BLS12_FP) return (false, "");
        return _bls12(address(0x10), element, BLS12_G1);
    }

    /// @dev The point of G2 an extension field element, as its two base field
    /// elements, maps to. Fails when either is not below the field modulus.
    function bls12MapFp2ToG2(bytes memory element)
        internal
        view
        returns (bool success, bytes memory result)
    {
        if (element.length != 2 * BLS12_FP) return (false, "");
        return _bls12(address(0x11), element, BLS12_G2);
    }

    /// @dev Whether `proof` shows that the polynomial the KZG `commitment`
    /// commits to, which `versionedHash` names, takes the value `y` at `z`
    /// (EIP-4844, from Cancun). The commitment and the proof are 48-byte
    /// compressed points of G1. The precompile fails for a false proof and
    /// for malformed input alike, and a chain without it answers with nothing,
    /// so each is `false`.
    function pointEvaluation(
        bytes32 versionedHash,
        uint256 z,
        uint256 y,
        bytes memory commitment,
        bytes memory proof
    ) internal view returns (bool) {
        if (commitment.length != 48 || proof.length != 48) return false;
        (bool success, bytes memory output) =
            address(0x0a).staticcall(abi.encodePacked(versionedHash, z, y, commitment, proof));
        if (!success || output.length != 64) return false;
        (uint256 elements, uint256 modulus) = abi.decode(output, (uint256, uint256));
        return elements == BLOB_FIELD_ELEMENTS && modulus == BLS12_MODULUS;
    }

    /// @dev Calls the BLS12-381 precompile at `precompile` with `input` and
    /// requires an answer of `size` bytes.
    function _bls12(address precompile, bytes memory input, uint256 size)
        private
        view
        returns (bool success, bytes memory result)
    {
        (success, result) = precompile.staticcall(input);
        if (!success || result.length != size) return (false, "");
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
