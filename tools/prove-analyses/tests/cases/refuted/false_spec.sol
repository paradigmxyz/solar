// A false source specification: `half(0)` returns 0, which is not below 0.
contract FalseSpec {
    /// @custom:ensures r < x
    function half(uint8 x) external pure returns (uint8 r) {
        return x / 2;
    }
}
