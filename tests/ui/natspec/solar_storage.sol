contract StorageTags {
    /// @custom:solar-storage
    //~^ ERROR: `@custom:solar-storage` must name a layout
    uint64[] missing;

    /// @custom:solar-storage inlien
    //~^ ERROR: `@custom:solar-storage` must name a layout
    uint64[] misspelled;

    /// @custom:solar-storage bitmap extra
    //~^ ERROR: `@custom:solar-storage bitmap` takes no arguments
    mapping(uint256 => bool) arguments;

    /// @custom:solar-storage inline
    /// @custom:solar-storage inline
    //~^ ERROR: duplicate `@custom:solar-storage` tag
    uint64[] duplicate;

    /// @custom:solar-inline
    //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
    uint64[] oldInline;

    /// @custom:solar-fuse account
    //~^ ERROR: unknown tag `@custom:solar-fuse`
    mapping(address => uint128) oldFuse;

    /// @custom:solar-bitmap
    //~^ ERROR: unknown tag `@custom:solar-bitmap`
    mapping(uint256 => bool) oldBitmap;

    /// @custom:solar-handle id dictionary
    //~^ ERROR: unknown tag `@custom:solar-handle`
    struct OldHandle {
        bytes32 id;
    }

    /// @custom:solar-storage inline
    //~^ ERROR: `@custom:solar-storage inline` must document a dynamic storage array state variable
    function misplaced() internal pure {}

    function statement() external pure returns (uint256 result) {
        /// @custom:solar-storage bitmap
        //~^ ERROR: `@custom:solar-storage` must document a storage state variable or a struct declared in a contract
        result = 1;
    }
}
