// `@custom:solar-bitmap` keeps a mapping from an unsigned integer to `bool` in
// bits, 256 keys to a word. Other key and value types are rejected, and the
// mapping may only be indexed.
contract Bitmap {
    /// @custom:solar-bitmap
    mapping(uint256 => bool) claimed;

    /// @custom:solar-bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(address => bool) byAddress;

    /// @custom:solar-bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(int256 => bool) signed;

    /// @custom:solar-bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(uint256 => uint8) counts;

    /// @custom:solar-bitmap
    //~^ ERROR: `@custom:solar-bitmap` must document a mapping state variable
    bool[] notMapping;

    function isSet(mapping(uint256 => bool) storage m, uint256 id) internal view returns (bool) {
        return m[id];
    }

    function passing(uint256 id) external view returns (bool) {
        return isSet(claimed, id); //~ ERROR: a bitmap mapping can only be indexed
    }

    function slot() external pure returns (uint256 s) {
        assembly {
            s := claimed.slot //~ ERROR: inline assembly cannot take the `.slot` of a bitmap mapping
        }
    }

    function update(uint256 id) external returns (bool) {
        claimed[id] = !claimed[id];
        delete claimed[id + 1];
        return claimed[id];
    }
}
