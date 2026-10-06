contract WideLayout {
    struct Wide {
        uint256[1 << 251] values;
        uint8 tail;
    }
    Wide private wide;
    uint8[(1 << 251) + 1] private packed;
    uint256 private afterPacked;
}
