// Arrays documented `@custom:solar-inline` appear in the storage layout under
// `inline`, with the entries `storage` gives them.
contract Lists {
    uint256 total;
    /// @custom:solar-inline
    uint64[] small;
    uint64[] plain;
    /// @custom:solar-inline
    bytes3[] codes;
}

contract Plain {
    uint64[] plain;
}
