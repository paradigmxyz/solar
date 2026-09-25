// `@custom:solar-view` on an internal function names parameters that are views
// of the caller's bytes: each must be a `bytes memory` or `string memory`
// parameter of the function.
contract Test {
    /// @custom:solar-view data text
    function fine(bytes memory data, string memory text) internal pure returns (uint256) {
        return data.length + bytes(text).length;
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

    /// @custom:solar-view values
    function words(uint256[] memory values) internal pure returns (uint256) { //~ ERROR: the view parameter `values` must be `bytes memory` or `string memory`
        return values.length;
    }

    /// @custom:solar-view data
    //~^ ERROR: `@custom:solar-view` must document a variable declaration statement or an internal function
    function external_(bytes calldata data) external pure returns (uint256) {
        return data.length;
    }
}
