contract ValidVariableTags {
    /// @return The number of decimals
    uint8 public decimals;
}

contract InvalidVariableTags {
    /// @return Invalid return on private variable
    //~^ ERROR: tag `@return` not valid for variables
    uint private privateVariable;

    /// @param owner Invalid parameter on variable
    //~^ ERROR: tag `@param` not valid for variables
    mapping(address owner => uint) public balanceOf;
}
