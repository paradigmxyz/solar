//@compile-flags: -Zprint-natspec

contract OverloadBase {
    /// @notice address overload
    //~^ NOTE: @notice address overload
    /// @param a address parameter
    //~^ NOTE: @param a address parameter
    function overloaded(address a) public virtual {}
    //~^ ERROR: resolved NatSpec for function

    /// @notice uint overload
    //~^ NOTE: @notice uint overload
    /// @param a uint parameter
    //~^ NOTE: @param a uint parameter
    function overloaded(uint a) public virtual {}
    //~^ ERROR: resolved NatSpec for function
}

contract OverloadChild is OverloadBase {
    /// @inheritdoc OverloadBase
    function overloaded(uint a) public override {}
    //~^ ERROR: resolved NatSpec for function
}
