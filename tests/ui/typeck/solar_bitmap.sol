// `@custom:solar-storage bitmap` keeps a mapping from an unsigned integer to `bool` in
// bits, 256 keys to a word. Other key and value types are rejected, and the
// mapping may only be indexed.
contract Bitmap {
    /// @custom:solar-storage bitmap
    mapping(uint256 => bool) claimed;

    /// @custom:solar-storage bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(address => bool) byAddress;

    /// @custom:solar-storage bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(int256 => bool) signed;

    /// @custom:solar-storage bitmap
    //~^ ERROR: a bitmap mapping must map an unsigned integer to `bool`
    mapping(uint256 => uint8) counts;

    /// @custom:solar-storage bitmap
    //~^ ERROR: `@custom:solar-storage bitmap` must document a mapping state variable
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
