//@compile-flags: -Zprint-natspec

contract ReturnDocs {
    /// @return the value
    //~^ NOTE: @return the value
    function unnamedReturn() public pure returns (uint) { //~ ERROR: resolved NatSpec for function
        return 1;
    }

    /// @return result The value
    //~^ NOTE: @return result The value
    function namedReturn() public pure returns (uint result) { //~ ERROR: resolved NatSpec for function
        return 1;
    }
}
