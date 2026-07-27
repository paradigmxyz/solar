//@compile-flags: -Zcodegen -Zdump=mir

// The prologue rebuilds a calldata struct in memory. A memory copy stands in
// for a whole-member read — length, hashing, encoding — because a
// `bytes`/`T[] calldata` view is read-only and the bytes agree. It does not
// stand in for element addressing: the copy of an array of structs holds
// pointers where calldata holds the elements inline, and even a word-element
// copy is addressed from the object rather than its data. Indexing such a
// member is rejected instead of answering wrongly.

struct Item {
    uint8 itemType;
    uint256 amount;
}

struct Params {
    address who;
    Item[] items;
    uint256[] words;
    bytes extra;
}

contract CalldataStructMemberIndex {
    // Whole-member reads keep working off the memory copy.
    function lengths(Params calldata p) external pure returns (uint256, uint256, uint256) {
        return (p.items.length, p.words.length, p.extra.length);
    }

    function hashExtra(Params calldata p) external pure returns (bytes32) {
        return keccak256(p.extra);
    }

    function structElement(Params calldata p) external pure returns (uint256) {
        return p.items[0].amount; //~ ERROR: codegen does not support indexing a dynamic array member of a calldata struct yet
    }

    function wordElement(Params calldata p) external pure returns (uint256) {
        return p.words[0]; //~ ERROR: codegen does not support indexing a dynamic array member of a calldata struct yet
    }
}
