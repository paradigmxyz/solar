// `@custom:solar-view` on an internal function names parameters that are views
// of the caller's values: each must be a parameter of the function of a memory
// reference type.
contract Test {
    struct Pair {
        uint256 a;
        bytes b;
    }

    /// @custom:solar-view data text values pair
    function fine(bytes memory data, string memory text, uint256[] memory values, Pair memory pair)
        internal
        pure
        returns (uint256)
    {
        return data.length + bytes(text).length + values.length + pair.b.length;
    }

    /// @custom:solar-view
    //~^ ERROR: `@custom:solar-view` on a function must name its view parameters
    function unnamed(bytes memory data) internal pure returns (uint256) {
        return data.length;
    }

    /// @custom:solar-view missing
    //~^ ERROR: `@custom:solar-view` names `missing`, which is not a parameter of `unknown`
    function unknown(bytes memory data) internal pure returns (uint256) {
        return data.length;
    }

    /// @custom:solar-view value
    function word(uint256 value) internal pure returns (uint256) { //~ ERROR: the view parameter `value` must be a memory reference
        return value;
    }

    /// @custom:solar-view data
    //~^ ERROR: `@custom:solar-view` must document a variable declaration statement or an internal function
    function external_(bytes calldata data) external pure returns (uint256) {
        return data.length;
    }
}
