// Each kind of item accepts the tags solc accepts on it.

/// @return Contracts have no return values.
//~^ WARN: tag `@return` not valid for contracts
/// @inheritdoc Base
//~^ WARN: tag `@inheritdoc` not valid for contracts
/// @src 0:1:2
//~^ WARN: tag `@src` not valid for contracts
contract Base {
    /// @author Constructors accept `@author`.
    /// @param a The parameter.
    constructor(uint a) {}

    function f() public virtual {}
}

contract Derived is Base {
    /// @inheritdoc Base
    //~^ WARN: tag `@inheritdoc` not valid for constructors
    constructor() Base(1) {}

    /// @solidity memory-safe-assembly
    //~^ WARN: tag `@solidity` not valid for functions
    function f() public override {}
}

/// @dev File-level variables accept `@dev`.
/// @custom:note And custom tags.
uint constant DEV = 1;

/// @notice But not `@notice`.
//~^ WARN: tag `@notice` not valid for file-level variables
/// @inheritdoc Base
//~^ WARN: tag `@inheritdoc` not valid for file-level variables
uint constant NOTICE = 1;

// solc does not validate the documentation of structs, enums and UDVTs.

/// @title Struct
/// @param missing Not a field.
/// @return Nothing.
/// @inheritdoc Base
struct S {
    uint a;
}

/// @param missing Not a value.
/// @return Nothing.
/// @inheritdoc Base
enum E {
    A
}

/// @title UDVT
/// @author Someone
/// @return Nothing.
type U is uint;
