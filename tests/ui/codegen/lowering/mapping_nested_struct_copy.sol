//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=STRUCT

// Mapping values that are structs use a runtime-computed base slot. Copy the
// complete value in both directions, following nested memory-struct pointers,
// and clear every occupied slot on delete. Runtime-verified against the Lil
// Fractional and Nitro cold-path scenarios in solidity-compiler-benchmarks.
contract MappingNestedStructCopy {
    struct Inner {
        uint256 left;
        uint256 right;
    }

    struct Outer {
        uint256 head;
        Inner inner;
        uint256 tail;
    }

    mapping(uint256 => Outer) internal values;

    // STRUCT-LABEL: fn @set
    // STRUCT: = mapping_slot
    // STRUCT: memory_to_storage struct<word, struct<word, word>, word>
    function set(uint256 key, uint256 head, uint256 left, uint256 right, uint256 tail) external {
        values[key] = Outer(head, Inner(left, right), tail);
    }

    // STRUCT-LABEL: fn @get
    // STRUCT: = mapping_slot
    // STRUCT: storage_to_memory struct<word, struct<word, word>, word>
    function get(uint256 key)
        external
        view
        returns (uint256 head, uint256 left, uint256 right, uint256 tail)
    {
        Outer memory value = values[key];
        return (value.head, value.inner.left, value.inner.right, value.tail);
    }

    // STRUCT-LABEL: fn @clear
    // STRUCT: = mapping_slot
    // STRUCT: clear_storage struct<word, struct<word, word>, word>
    function clear(uint256 key) external {
        delete values[key];
    }
}
