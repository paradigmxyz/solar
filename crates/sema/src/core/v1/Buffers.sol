// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Arrays} from "solar:core/v1/Arrays.sol";
import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice Output being assembled. Its fields belong to `Buffers`; read the
/// result through `finish`.
struct ByteBuilder {
    bytes data;
    uint256 used;
}

/// @notice Words being assembled into a `uint256[]`. Its fields belong to
/// `Buffers`; read the result through `finish`.
struct WordBuilder {
    uint256[] data;
    uint256 used;
}

/// @notice Addresses being assembled into an `address[]`. Its fields belong to
/// `Buffers`; read the result through `finish`.
struct AddressBuilder {
    address[] data;
    uint256 used;
}

/// @notice Words being assembled into a `bytes32[]`. Its fields belong to
/// `Buffers`; read the result through `finish`.
struct Bytes32Builder {
    bytes32[] data;
    uint256 used;
}

/// @notice Signed words being assembled into an `int256[]`. Its fields belong
/// to `Buffers`; read the result through `finish`.
struct Int256Builder {
    int256[] data;
    uint256 used;
}

/// @notice Builders for output whose length is not known until it is written.
/// @dev Compiler-owned module, imported as `solar:core/v1/Buffers.sol`. The
/// capacity given to `create` is a hint: an append that does not fit
/// allocates at least twice the space and moves what was written, so appends
/// never write outside the builder and never fail for lack of room. `finish`
/// shortens the backing bytes to what was written and returns them without a
/// copy, then empties the builder, so nothing appended afterwards can reach
/// the bytes that were returned. A `WordBuilder` is the same over a
/// `uint256[]`, and an `AddressBuilder`, a `Bytes32Builder` and an
/// `Int256Builder` over arrays of those types. This is library code over
/// `Bytes.copyInto` and `Arrays.truncate`.
///
/// A builder is finished once, after its last append. This compiler rejects
/// any use of a builder after `finish` emptied it, any read or write of a
/// builder's fields outside this module, and any encoding or storing of a
/// whole builder, all of which could expose bytes that were never written;
/// other compilers run such code as described above. Since no code can read a
/// builder's capacity past what was written, this compiler also leaves the
/// memory behind it as it finds it rather than zeroing it first; `finish` cuts
/// the result off at what was written.
library Buffers {
    /// @dev A builder with room for `capacity` bytes before it first grows.
    function create(uint256 capacity) internal pure returns (ByteBuilder memory builder) {
        builder.data = _backing(capacity);
    }

    /// @dev How many bytes have been appended.
    function length(ByteBuilder memory builder) internal pure returns (uint256) {
        return builder.used;
    }

    /// @dev Appends `chunk`.
    function append(ByteBuilder memory builder, bytes memory chunk) internal pure {
        uint256 used = builder.used;
        uint256 needed = used + chunk.length;
        if (needed > builder.data.length) _grow(builder, needed);
        Bytes.copyInto(builder.data, used, chunk, 0, chunk.length);
        builder.used = needed;
    }

    /// @dev Appends one byte.
    function appendByte(ByteBuilder memory builder, bytes1 value) internal pure {
        uint256 used = builder.used;
        if (used == builder.data.length) _grow(builder, used + 1);
        Bytes.writeBytes1(builder.data, used, value);
        builder.used = used + 1;
    }

    /// @dev The bytes appended so far. The builder is empty afterwards.
    function finish(ByteBuilder memory builder) internal pure returns (bytes memory result) {
        result = builder.data;
        Arrays.truncate(result, builder.used);
        builder.data = "";
        builder.used = 0;
    }

    /// @dev A word builder with room for `capacity` words before it first
    /// grows.
    function createWords(uint256 capacity) internal pure returns (WordBuilder memory builder) {
        builder.data = _wordBacking(capacity);
    }

    /// @dev How many words have been appended.
    function length(WordBuilder memory builder) internal pure returns (uint256) {
        return builder.used;
    }

    /// @dev Appends one word.
    function append(WordBuilder memory builder, uint256 value) internal pure {
        uint256 used = builder.used;
        if (used == builder.data.length) _growWords(builder, used + 1);
        builder.data[used] = value;
        builder.used = used + 1;
    }

    /// @dev The words appended so far. The builder is empty afterwards.
    function finish(WordBuilder memory builder) internal pure returns (uint256[] memory result) {
        result = builder.data;
        Arrays.truncate(result, builder.used);
        builder.data = new uint256[](0);
        builder.used = 0;
    }

    /// @dev A builder of addresses with room for `capacity` of them before it
    /// first grows.
    function createAddresses(uint256 capacity) internal pure returns (AddressBuilder memory builder) {
        builder.data = _addressBacking(capacity);
    }

    /// @dev How many addresses have been appended.
    function length(AddressBuilder memory builder) internal pure returns (uint256) {
        return builder.used;
    }

    /// @dev Appends `value`.
    function append(AddressBuilder memory builder, address value) internal pure {
        uint256 used = builder.used;
        if (used == builder.data.length) {
            uint256 capacity = used * 2;
            if (capacity == 0) capacity = 1;
            address[] memory grown = _addressBacking(capacity);
            for (uint256 i; i < used; ++i) {
                grown[i] = builder.data[i];
            }
            builder.data = grown;
        }
        builder.data[used] = value;
        builder.used = used + 1;
    }

    /// @dev The addresses appended so far. The builder is empty afterwards.
    function finish(AddressBuilder memory builder) internal pure returns (address[] memory result) {
        result = builder.data;
        Arrays.truncate(result, builder.used);
        builder.data = new address[](0);
        builder.used = 0;
    }

    /// @dev A builder of words with room for `capacity` of them before it
    /// first grows.
    function createBytes32s(uint256 capacity) internal pure returns (Bytes32Builder memory builder) {
        builder.data = _bytes32Backing(capacity);
    }

    /// @dev How many words have been appended.
    function length(Bytes32Builder memory builder) internal pure returns (uint256) {
        return builder.used;
    }

    /// @dev Appends `value`.
    function append(Bytes32Builder memory builder, bytes32 value) internal pure {
        uint256 used = builder.used;
        if (used == builder.data.length) {
            uint256 capacity = used * 2;
            if (capacity == 0) capacity = 1;
            bytes32[] memory grown = _bytes32Backing(capacity);
            for (uint256 i; i < used; ++i) {
                grown[i] = builder.data[i];
            }
            builder.data = grown;
        }
        builder.data[used] = value;
        builder.used = used + 1;
    }

    /// @dev The words appended so far. The builder is empty afterwards.
    function finish(Bytes32Builder memory builder) internal pure returns (bytes32[] memory result) {
        result = builder.data;
        Arrays.truncate(result, builder.used);
        builder.data = new bytes32[](0);
        builder.used = 0;
    }

    /// @dev A builder of signed words with room for `capacity` of them before it
    /// first grows.
    function createInt256s(uint256 capacity) internal pure returns (Int256Builder memory builder) {
        builder.data = _int256Backing(capacity);
    }

    /// @dev How many signed words have been appended.
    function length(Int256Builder memory builder) internal pure returns (uint256) {
        return builder.used;
    }

    /// @dev Appends `value`.
    function append(Int256Builder memory builder, int256 value) internal pure {
        uint256 used = builder.used;
        if (used == builder.data.length) {
            uint256 capacity = used * 2;
            if (capacity == 0) capacity = 1;
            int256[] memory grown = _int256Backing(capacity);
            for (uint256 i; i < used; ++i) {
                grown[i] = builder.data[i];
            }
            builder.data = grown;
        }
        builder.data[used] = value;
        builder.used = used + 1;
    }

    /// @dev The signed words appended so far. The builder is empty afterwards.
    function finish(Int256Builder memory builder) internal pure returns (int256[] memory result) {
        result = builder.data;
        Arrays.truncate(result, builder.used);
        builder.data = new int256[](0);
        builder.used = 0;
    }

    /// @dev Backing for a byte builder. The body zeroes it, as `new` does;
    /// this compiler leaves the bytes as they are, which no code can read.
    function _backing(uint256 length) private pure returns (bytes memory) {
        return new bytes(length);
    }

    /// @dev Backing for a `WordBuilder`, as `_backing`.
    function _wordBacking(uint256 length) private pure returns (uint256[] memory) {
        return new uint256[](length);
    }

    /// @dev Backing for an `AddressBuilder`, as `_backing`.
    function _addressBacking(uint256 length) private pure returns (address[] memory) {
        return new address[](length);
    }

    /// @dev Backing for a `Bytes32Builder`, as `_backing`.
    function _bytes32Backing(uint256 length) private pure returns (bytes32[] memory) {
        return new bytes32[](length);
    }

    /// @dev Backing for an `Int256Builder`, as `_backing`.
    function _int256Backing(uint256 length) private pure returns (int256[] memory) {
        return new int256[](length);
    }

    /// @dev Moves the written words into an allocation of at least `needed`.
    function _growWords(WordBuilder memory builder, uint256 needed) private pure {
        uint256 capacity = builder.data.length * 2;
        if (capacity < needed) capacity = needed;
        uint256[] memory grown = _wordBacking(capacity);
        for (uint256 i; i < builder.used; ++i) {
            grown[i] = builder.data[i];
        }
        builder.data = grown;
    }

    /// @dev Moves the written bytes into an allocation of at least `needed`.
    function _grow(ByteBuilder memory builder, uint256 needed) private pure {
        uint256 capacity = builder.data.length * 2;
        if (capacity < needed) capacity = needed;
        bytes memory grown = _backing(capacity);
        Bytes.copyInto(grown, 0, builder.data, 0, builder.used);
        builder.data = grown;
    }
}
