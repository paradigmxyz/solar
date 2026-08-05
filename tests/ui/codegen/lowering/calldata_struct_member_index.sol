//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CDSMI

// Indexing a calldata array whose elements are wider than a word. The elements
// are laid out by the ABI rules for the element type, so an index strides by the
// element's head size and rebuilds it; only a word element sits inline in one
// slot and loads directly.
//
// Struct members stay in calldata. Indexing computes the ABI head offset and
// decodes the selected element without copying the whole member.
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
    // A struct element strides by its head size (four words), not by one.
    // CDSMI-LABEL: fn @plain
    // CDSMI: shl 7, arg1
    function plain(Item[] calldata items, uint256 i) external pure returns (uint256) {
        return items[i].amount;
    }

    // The member uses its four-word ABI head stride.
    // CDSMI-LABEL: fn @member
    // CDSMI: shl 7, arg1
    // CDSMI: calldataload
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
}
