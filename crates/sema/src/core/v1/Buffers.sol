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

/// @notice Builders for output whose length is not known until it is written.
/// @dev Compiler-owned module, imported as `solar:core/v1/Buffers.sol`. The
/// capacity given to `create` is a hint: an append that does not fit
/// allocates at least twice the space and moves what was written, so appends
/// never write outside the builder and never fail for lack of room. `finish`
/// shortens the backing bytes to what was written and returns them without a
/// copy, then empties the builder, so nothing appended afterwards can reach
/// the bytes that were returned. This is library code over `Bytes.copyInto`
/// and `Arrays.truncate`.
library Buffers {
    /// @dev A builder with room for `capacity` bytes before it first grows.
    function create(uint256 capacity) internal pure returns (ByteBuilder memory builder) {
        builder.data = new bytes(capacity);
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

    /// @dev Moves the written bytes into an allocation of at least `needed`.
    function _grow(ByteBuilder memory builder, uint256 needed) private pure {
        uint256 capacity = builder.data.length * 2;
        if (capacity < needed) capacity = needed;
        bytes memory grown = new bytes(capacity);
        Bytes.copyInto(grown, 0, builder.data, 0, builder.used);
        builder.data = grown;
    }
}
