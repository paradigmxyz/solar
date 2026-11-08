// ported-from: test/libsolidity/natspecJSON/dev_constructor_return.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_author_title_state_variable.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_inherit_modifier_no_return.sol
// ported-from: test/libsolidity/syntaxTests/constants/constant_natspec_user.sol

contract ConstructorReturn {
    /// @param a the parameter a is really nice and very useful
    /// @param second the second parameter is not very useful, it just provides additional confusion
    /// @return return should not work within constructors
    //~^ WARN: tag `@return` not valid for constructors
    constructor(uint a, uint second) { }
}

contract AuthorTitleStateVariable {
    /// @title title
    //~^ WARN: tag `@title` not valid for non-public state variables
    /// @author author
    //~^ WARN: tag `@author` not valid for non-public state variables
    uint private state;
}

contract ModifierReturn {
	///@return
	//~^ WARN: tag `@return` not valid for modifiers
	modifier m22 virtual { _; }
}

contract ModifierReturnOverride is ModifierReturn {
	modifier m22 override { _; }
}

/// Documentation
//~^ WARN: tag `@notice` not valid for file-level variables
uint constant x = 8;
