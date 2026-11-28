pragma solidity ^0.8.24;
// `mcopy` of a constant length from a `bytes` value at a symbolic offset into another: the
// copied bytes are the source's, provided both ranges lie within the objects' contents.
contract Copy {
    /// @custom:requires off + 96 <= data.length && data.length < 2 ** 64
    /// @custom:ensures key.length == 96
    /// @custom:ensures forall uint256 i :: i < 96 ==> key[i] == data[off + i]
    function ok_copy(bytes memory data, uint256 off) external pure returns (bytes memory key) {
        key = new bytes(96);
        assembly {
            mcopy(add(key, 32), add(add(data, 32), off), 96)
        }
    }

    // Off by one: byte 95 comes from data[off + 95].
    /// @custom:requires off + 96 <= data.length && data.length < 2 ** 64
    /// @custom:ensures key[95] == data[off + 96]
    function bad_copy_off_by_one(bytes memory data, uint256 off)
        external
        pure
        returns (bytes memory key)
    {
        key = new bytes(96);
        assembly {
            mcopy(add(key, 32), add(add(data, 32), off), 96)
        }
    }

    // The source range may run past the end of `data`.
    // expect: may read outside the contents of `data`
    /// @custom:ensures key.length == 96
    function bad_copy_out_of_bounds(bytes memory data, uint64 off)
        external
        pure
        returns (bytes memory key)
    {
        key = new bytes(96);
        assembly {
            mcopy(add(key, 32), add(add(data, 32), off), 96)
        }
    }

    // A copy past the end of the destination.
    // expect: may write outside the contents of `key`
    /// @custom:requires data.length >= 96
    /// @custom:ensures key.length == 64
    function bad_copy_dest_too_short(bytes memory data) external pure returns (bytes memory key) {
        key = new bytes(64);
        assembly {
            mcopy(add(key, 32), add(data, 32), 96)
        }
    }

    // expect: non-constant length
    /// @custom:requires n <= data.length && n <= 96
    /// @custom:ensures key.length == 96
    function bad_copy_symbolic_len(bytes memory data, uint256 n)
        external
        pure
        returns (bytes memory key)
    {
        key = new bytes(96);
        assembly {
            mcopy(add(key, 32), add(data, 32), n)
        }
    }

    // expect: copies inside one object are not modelled
    /// @custom:requires data.length >= 64
    /// @custom:ensures data.length >= 64
    function bad_copy_overlap(bytes memory data) external pure returns (bytes memory) {
        assembly {
            mcopy(add(data, 32), add(data, 64), 32)
        }
        return data;
    }

    // expect: is not in a Solidity `bytes` or `string` variable
    /// @custom:requires data.length >= 32
    /// @custom:ensures data.length >= 32
    function bad_copy_unknown_dest(bytes memory data) external pure returns (bytes memory) {
        assembly {
            mcopy(0, add(data, 32), 32)
        }
        return data;
    }

    // The destination shares its memory with `other`, which the copy changes too.
    // expect: aliased memory is not modelled
    /// @custom:requires data.length >= 96
    /// @custom:ensures r == 0
    function bad_copy_alias(bytes memory data) external pure returns (uint8 r) {
        bytes memory key = new bytes(96);
        bytes memory other = key;
        assembly {
            mcopy(add(key, 32), add(data, 32), 32)
        }
        r = uint8(other[0]);
    }

    // A destination reached through a block variable is not seen as a write to `key`.
    // expect: destination must name `key` itself
    /// @custom:requires data.length >= 32
    /// @custom:ensures key.length == 32
    function bad_copy_through_local(bytes memory data) external pure returns (bytes memory key) {
        key = new bytes(32);
        assembly {
            let p := key
            mcopy(add(p, 32), add(data, 32), 32)
        }
    }

    // Assembly that points a reference at other memory makes it share: a later copy into it
    // is refused.
    // expect: may share its memory with another reference
    /// @custom:requires data.length >= 32
    /// @custom:ensures key.length == 32
    function bad_copy_after_asm_alias(bytes memory data) external pure returns (bytes memory key) {
        key = new bytes(32);
        assembly {
            key := data
        }
        assembly {
            mcopy(add(key, 32), add(data, 32), 32)
        }
    }
}
