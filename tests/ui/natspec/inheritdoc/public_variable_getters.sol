//@compile-flags: -Zprint-natspec

interface Base {
    /// @notice Base factory notice
    //~^ NOTE: @notice Base factory notice
    /// @return The factory address
    //~^ NOTE: @return The factory address
    function factory() external view returns (address);
    //~^ ERROR: resolved NatSpec for function `Base.factory`

    /// @notice Base value notice
    //~^ NOTE: @notice Base value notice
    /// @param key The key to look up
    //~^ NOTE: @param key The key to look up
    /// @return The stored value
    //~^ NOTE: @return The stored value
    function values(uint key) external view returns (uint);
    //~^ ERROR: resolved NatSpec for function `Base.values`
}

contract Child is Base {
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.factory()`
    address public override factory;
    //~^ ERROR: resolved NatSpec for variable `Child.factory`

    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.values(uint256)`
    mapping(uint => uint) public override values;
    //~^ ERROR: resolved NatSpec for variable `Child.values`
}
