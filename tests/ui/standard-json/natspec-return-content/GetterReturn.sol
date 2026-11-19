interface I {
    /// @return Units
    function inherited(address who) external view returns (uint256);
    /// @return
    function inheritedEmpty(address who) external view returns (uint256);
}

contract C is I {
    mapping(address who => uint256 fresh) public override inherited;
    mapping(address who => uint256 fresh) public override inheritedEmpty;
    struct P {
        uint256 x;
        uint256 y;
    }
    /// @return x the x
    P public named;
    /// @return x
    P public nameOnly;
    /// @return x x
    P public sameAsName;
    /// @return plain
    uint256 public unnamed;
}
