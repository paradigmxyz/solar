// Mappings documented `@custom:solar-bitmap` appear in the storage layout under
// `bitmaps`, with the entries `storage` gives them.
contract Claims {
    uint256 total;
    /// @custom:solar-bitmap
    mapping(uint256 => bool) claimed;
    mapping(uint256 => bool) plain;
}

contract Plain {
    mapping(uint256 => bool) plain;
}
