contract Writes {
    /// @custom:solar-storage handle id dictionary
    struct Item { uint256 id; }
    struct Outer { Item inner; }
    uint256[] dictionary;
    uint256[] other;
    /// @custom:solar-storage inline
    uint64[] packed;
    uint64[] plain;
    Outer outer;
    mapping(uint256 => Outer) records;

    function mutations(uint256[] memory replacement) external {
        dictionary[0]++; //~ ERROR: a handle dictionary is append-only
        ++dictionary[0]; //~ ERROR: a handle dictionary is append-only
        dictionary[0]--; //~ ERROR: a handle dictionary is append-only
        --dictionary[0]; //~ ERROR: a handle dictionary is append-only
        uint256 dummy;
        (dictionary, dummy) = (replacement, 0); //~ ERROR: a handle dictionary is append-only
        uint256[] storage alias_ = other;
        (alias_, dummy) = (dictionary, 0); //~ ERROR: a handle dictionary cannot be a storage reference
        uint256[](dictionary)[0] = 9; //~ ERROR: a handle dictionary cannot be a storage reference
        uint64[] storage inlineAlias = plain;
        (inlineAlias, dummy) = (packed, 0); //~ ERROR: an inline array cannot be a storage reference
        uint64[](packed)[0] = 9; //~ ERROR: an inline array cannot be a storage reference
    }

    function replaceInline(uint64[] memory replacement) external {
        uint256 dummy;
        (packed, dummy) = (replacement, 0); //~ ERROR: an inline array cannot be assigned as a whole
    }

    function whole(Item memory value) external {
        outer.inner = value; //~ ERROR: a struct with handles cannot be written to storage as a whole
        Outer storage p = outer;
        p.inner = value; //~ ERROR: a struct with handles cannot be written to storage as a whole
        records[0].inner = value; //~ ERROR: a struct with handles cannot be written to storage as a whole
    }
}
