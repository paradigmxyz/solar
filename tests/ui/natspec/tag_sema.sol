/// @title A valid contract with documentation
/// @author Solar Team
/// @notice This is a notice for users
/// @dev This is a dev note
/// @custom:security-contact security@example.com
contract ValidContract {
    function foo() public {}
}

contract ValidItems {
    /// @title State enumeration
    /// @notice Possible states of the contract
    enum State { Created, Locked, Inactive }

    /// @notice Emitted when tokens are transferred
    /// @dev This follows ERC20 standard
    /// @param from The sender address
    /// @param to The recipient address
    /// @param amount The amount transferred
    /// @custom:indexed-params 2
    event Transfer(address indexed from, address indexed to, uint amount);

    /// @title User information
    /// @notice Contains user data
    /// @dev Stored in mapping
    struct User {
        address addr;
        uint balance;
    }

    /// @notice Transfer tokens to another address
    /// @dev Implements ERC20 transfer
    /// @param to The recipient address
    /// @param amount The amount to transfer
    /// @return success Whether the transfer succeeded
    /// @custom:throws InsufficientBalance
    function transfer(address to, uint amount) public returns (bool success) {
        return true;
    }
}

// -- ERROR TESTS - DUPLICATE TAGS ---------------------------------------------

/// @author First author
/// @author Second author
contract DuplicateAuthor {}

/// @title First title
/// @title Second title
//~^ ERROR: tag @title can only be given once
contract DuplicateTitle {}

contract DuplicateParamBase {
    /// @param x First documentation
    //~^ NOTE: previously documented here
    /// @param x Second documentation
    //~^ ERROR: duplicate documentation for parameter 'x'
    function foo(uint x) public {}
}

contract DuplicateInheritdocBase {
    function foo() public {}
}

contract DuplicateInheritdoc is DuplicateInheritdocBase {
    /// @inheritdoc DuplicateInheritdocBase
    /// @inheritdoc DuplicateInheritdocBase
    //~^ ERROR: tag @inheritdoc can only be given once
    function foo() public override {}
}

// -- ERROR TESTS - INVALID CONTEXT --------------------------------------------

contract InvalidTagContext {
    /// @author Invalid author on function
    //~^ ERROR: tag `@author` not valid for functions
    function invalidAuthor() public {}

    /// @title Invalid title on function
    //~^ ERROR: tag `@title` not valid for functions
    function invalidTitle() public {}

    /// @return Invalid return on event
    //~^ ERROR: tag `@return` not valid for events
    event InvalidReturn(address from, address to);
}

contract InvalidInheritdocBase {
    event Transfer(address from, address to);
}

contract InvalidInheritdoc is InvalidInheritdocBase {
    /// @inheritdoc InvalidInheritdocBase
    //~^ ERROR: tag `@inheritdoc` not valid for events
    event Transfer(address from, address to);
}

contract InvalidParamName {
    /// @param x Valid parameter
    /// @param y Invalid parameter name
    //~^ ERROR: tag `@param` references non-existent parameter 'y'
    function foo(uint x) public {}
}

contract TooManyReturns {
    /// @return First return value
    /// @return Second return value
    /// @return Third return value
    //~^ ERROR: too many `@return` tags: function has 2 return values, found 3
    function foo() public returns (uint, uint) {}
}
