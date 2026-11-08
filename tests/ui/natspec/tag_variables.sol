contract ValidVariableTags {
    /// @return The number of decimals
    uint8 public decimals;
}

contract InvalidVariableTags {
    /// @return Invalid return on private variable
    //~^ WARN: tag `@return` not valid for non-public state variables
    uint private privateVariable;

    /// @param owner Invalid parameter on variable
    //~^ WARN: tag `@param` not valid for public state variables
    mapping(address owner => uint) public balanceOf;
}
