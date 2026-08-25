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
    /// @param addr User address
    /// @param balance User balance
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
    function transfer(address to, uint amount) public pure returns (bool success) {
        return true;
    }
}

// -- WARNING TESTS - DUPLICATE TAGS -------------------------------------------

/// @author First author
/// @author Second author
contract DuplicateAuthor {}

/// @title First title
/// @title Second title
contract DuplicateTitle {}

contract DuplicateParamBase {
    // Duplicate `@param` tags are accepted, matching solc.
    /// @param x First documentation
    /// @param x Second documentation
    function foo(uint x) public {}
}

contract DuplicateInheritdocBase {
    function foo() public virtual {}
}

contract DuplicateInheritdoc is DuplicateInheritdocBase {
    /// @inheritdoc DuplicateInheritdocBase
    /// @inheritdoc DuplicateInheritdocBase
    //~^ WARN: tag @inheritdoc can only be given once
    function foo() public override {}
}

// -- WARNING TESTS - INVALID CONTEXT ------------------------------------------

contract InvalidTagContext {
    /// @author Invalid author on function
    //~^ WARN: tag `@author` not valid for functions
    function invalidAuthor() public {}

    /// @title Invalid title on function
    //~^ WARN: tag `@title` not valid for functions
    function invalidTitle() public {}

    /// @return Invalid return on event
    //~^ WARN: tag `@return` not valid for events
    event InvalidReturn(address from, address to);
}

contract InvalidInheritdocBase {
    event Transfer(address from, address to);
}

contract InvalidInheritdoc is InvalidInheritdocBase {
    /// @inheritdoc InvalidInheritdocBase
    //~^ WARN: tag `@inheritdoc` not valid for events
    event InvalidInheritdocEvent(address from, address to);
}

contract InvalidParamName {
    /// @param x Valid parameter
    /// @param y Invalid parameter name
    //~^ WARN: tag `@param` references non-existent parameter 'y'
    function foo(uint x) public {}
}

contract SelfInheritdoc {
    /// @inheritdoc SelfInheritdoc
    //~^ WARN: tag `@inheritdoc` references contract "SelfInheritdoc", which is not a base of this contract
    function foo() public {}
}

contract MissingInheritdocContract {
    /// @inheritdoc DoesNotExist
    //~^ WARN: tag `@inheritdoc` references inexistent contract "DoesNotExist"
    function foo() public pure {}
}

contract InheritdocGrandparent {
    function inherited() public virtual {}
}

contract InheritdocIntermediate is InheritdocGrandparent {}

contract InvalidIntermediateInheritdoc is InheritdocIntermediate {
    /// @inheritdoc InheritdocIntermediate
    //~^ WARN: tag `@inheritdoc` references contract "InheritdocIntermediate", but the contract does not contain a matching item that can be inherited
    function inherited() public override {}
}

contract StructParamDocs {
    /// @param value Valid field name
    /// @param value Duplicate struct field docs are accepted for solc compatibility
    /// @param missing Unknown struct field docs are accepted for solc compatibility
    struct User {
        uint value;
    }
}
