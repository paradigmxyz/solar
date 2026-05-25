//@compile-flags: -Zprint-natspec

contract VariableDocs {
    /// @return The number of decimals
    //~^ NOTE: @return The number of decimals
    uint8 public decimals;
    //~^ ERROR: resolved NatSpec for variable
}
