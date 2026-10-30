contract InlineForms {
    /// @custom:solar-inline
    uint64[] values;

    /// @custom:solar-inline always
    //~^ ERROR: `@custom:solar-inline` on a storage array takes no arguments
    uint64[] alwaysValues;

    /// @custom:solar-inline never
    //~^ ERROR: `@custom:solar-inline` on a storage array takes no arguments
    uint64[] neverValues;

    /// @custom:solar-inline
    function hint(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    /// @custom:solar-inline always
    function alwaysInline(uint256 x) internal pure returns (uint256) {
        return x + 2;
    }

    /// @custom:solar-inline never
    function neverInline(uint256 x) internal pure returns (uint256) {
        return x + 3;
    }

    function useForms(uint256 x) external view returns (uint256) {
        return hint(x) + alwaysInline(x) + neverInline(x) + values.length;
    }
}
