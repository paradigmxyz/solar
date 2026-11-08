//@ compile-flags: -Zprint-natspec

contract OverloadBase {
    /// @notice address overload
    /// @param a address parameter
    //~^^ NOTE: @notice address overload
    //~^^ NOTE: @param a address parameter
    function overloaded(address a) public virtual {}
    //~^ ERROR: resolved NatSpec for function `OverloadBase.overloaded`

    /// @notice uint overload
    /// @param a uint parameter
    //~^^ NOTE: @notice uint overload
    //~^^ NOTE: @param a uint parameter
    function overloaded(uint a) public virtual {}
    //~^ ERROR: resolved NatSpec for function `OverloadBase.overloaded`
}

contract OverloadChild is OverloadBase {
    /// @inheritdoc OverloadBase
    //~^ NOTE: inherits NatSpec from function `OverloadBase.overloaded(uint256)`
    function overloaded(uint a) public override {}
    //~^ ERROR: resolved NatSpec for function `OverloadChild.overloaded`
}
