contract ValidReturnTags {
    /// @return The unnamed return description
    function unnamedReturn() public returns (uint) {
        return 1;
    }

    /// @return result The named return description
    function namedReturn() public returns (uint result) {
        return 1;
    }
}

contract TooManyReturns {
    /// @return First return value
    /// @return Second return value
    /// @return Third return value
    //~^ ERROR: too many `@return` tags: function has 2 return values, found 3
    function tooManyReturns() public returns (uint, uint) {}
}

contract InvalidReturnNames {
    /// @return other Invalid return name
    //~^ ERROR: tag `@return` references non-existent return parameter 'other'
    function invalidName() public returns (uint result) {}

    /// @return
    //~^ ERROR: tag `@return` does not contain the name of its return parameter
    function missingName() public returns (uint result) {}
}

contract InvalidReturnOrder {
    /// @return second The second return value is documented first
    //~^ ERROR: mismatched `@return` parameter: expected `first`, found `second`
    /// @return first The first return value is documented second
    //~^ ERROR: mismatched `@return` parameter: expected `second`, found `first`
    function swappedNames() public returns (uint first, uint second) {}
}
