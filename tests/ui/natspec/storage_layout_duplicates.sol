contract DuplicateLayouts {
    /// @custom:solar-storage bitmap
    /// @custom:solar-storage fuse first
    //~^ ERROR: duplicate `@custom:solar-storage` tag
    mapping(uint256 => bool) firstA;
    /// @custom:solar-storage fuse first
    mapping(uint256 => bool) firstB;

    /// @custom:solar-storage fuse second
    /// @custom:solar-storage bitmap
    //~^ ERROR: duplicate `@custom:solar-storage` tag
    mapping(uint256 => bool) secondA;
    /// @custom:solar-storage fuse second
    mapping(uint256 => bool) secondB;
}
