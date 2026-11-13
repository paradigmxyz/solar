// ported-from: test/libsolidity/natspecJSON/dev_documenting_no_param_description.sol
// ported-from: test/libsolidity/natspecJSON/dev_documenting_no_param_name_end.sol
// ported-from: test/libsolidity/natspecJSON/dev_documenting_no_param_name.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_empty_description.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_empty_tag.sol

contract NoParamDescription {
    /// @dev Multiplies a number by 7 and adds second parameter
    /// @param a Documentation for the first parameter
    /// @param second
    //~^ WARN: tag `@param` does not describe parameter `second`
    function mul(uint a, uint second) public pure returns (uint d) { return a * 7 + second; }
}

contract NoParamNameEnd {
    /// @dev Multiplies a number by 7 and adds second parameter
    /// @param a Documentation for the first parameter
    /// @param se
    //~^ WARN: tag `@param` does not describe parameter `se`
    function mul(uint a, uint second) public pure returns (uint d) { return a * 7 + second; }
}

contract NoParamName {
    /// @dev Multiplies a number by 7 and adds second parameter
    /// @param a Documentation for the first parameter
    /// @param
    //~^ WARN: tag `@param` does not contain a parameter name
    function mul(uint a, uint second) public pure returns (uint d) { return a * 7 + second; }
}

contract EmptyDescription {
    /// @param id
    //~^ WARN: tag `@param` does not describe parameter `id`
    function vote(uint id) public {}
}

abstract contract EmptyTag {
    /// @param
    //~^ WARN: tag `@param` does not contain a parameter name
    function vote(uint id) public {}
}
