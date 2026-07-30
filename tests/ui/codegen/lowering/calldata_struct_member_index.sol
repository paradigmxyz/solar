//@run-call: sideEffectLength (0x0000000000000000000000000000000000000001, [(1, 0x0000000000000000000000000000000000000002, 3, 4)], [9], 0xaabb) => 1, 1
//@run-call: sideEffectArgument (0x0000000000000000000000000000000000000001, [(1, 0x0000000000000000000000000000000000000002, 3, 4)], [9], 0xaabb) => 1, 4
//@run-call: sliceFixed [[1, 2], [3, 4], [5, 6]] => 3, 4

// Indexing a calldata array whose elements are wider than a word. The elements
// are laid out by the ABI rules for the element type, so an index strides by the
// element's head size and rebuilds it; only a word element sits inline in one
// slot and loads directly.
//
// An array member of a calldata struct is rebuilt into its ordinary memory
// representation. Its element slots hold pointers to rebuilt structs, so
// indexing reuses the copy instead of decoding the same calldata again.
// Verified against solc on anvil.

struct Item {
    uint8 itemType;
    address token;
    uint256 identifier;
    uint256 amount;
}

struct Params {
    address who;
    Item[] items;
    uint256[] words;
    bytes extra;
}

contract CalldataStructMemberIndex {
    uint256 private calls;

    // A struct element strides by its head size (four words), not by one.
    // CDSMI-LABEL: fn @plain
    // CDSMI: shl 7, arg1
    function plain(Item[] calldata items, uint256 i) external pure returns (uint256) {
        return items[i].amount;
    }

    // The member uses the dynamic-memory-array layout and loads a struct pointer.
    // CDSMI-LABEL: fn @member
    // CDSMI: mul arg1, 32
    // CDSMI: mload
    function member(Params calldata p, uint256 i) external pure returns (uint256) {
        return p.items[i].amount;
    }

    // A word element still loads inline from `data + i * 32`.
    // CDSMI-LABEL: fn @word
    // CDSMI-NOT: shl 7, arg1
    function word(Params calldata p, uint256 i) external pure returns (uint256) {
        return p.words[i];
    }

    // Whole-member reads keep working off the memory copy.
    // CDSMI-LABEL: fn @lengths
    function lengths(Params calldata p) external pure returns (uint256, uint256, uint256) {
        return (p.items.length, p.words.length, p.extra.length);
    }

    function select(Params calldata p) internal returns (Params calldata) {
        calls++;
        return p;
    }

    function lastAmount(Item[] calldata items) internal pure returns (uint256) {
        return items[items.length - 1].amount;
    }

    function sideEffectLength(Params calldata p) external returns (uint256, uint256) {
        calls = 0;
        uint256 length = select(p).items.length;
        return (calls, length);
    }

    function sideEffectArgument(Params calldata p) external returns (uint256, uint256) {
        calls = 0;
        uint256 amount = lastAmount(select(p).items);
        return (calls, amount);
    }

    function sliceFixed(
        uint256[2][] calldata values
    ) external pure returns (uint256, uint256) {
        uint256[2][] calldata sliced = values[1:];
        return (sliced[0][0], sliced[0][1]);
    }
}
