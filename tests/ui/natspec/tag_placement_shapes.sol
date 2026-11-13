// Each kind of item accepts the tags solc accepts on it.

/// @return Contracts have no return values.
/// @inheritdoc Base
/// @src 0:1:2
//~^^^ WARN: tag `@return` not valid for contracts
//~^^^ WARN: tag `@inheritdoc` not valid for contracts
//~^^^ WARN: tag `@src` not valid for contracts
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

    /// @not-a-tag Unknown tags are not valid anywhere.
    //~^ WARN: tag `@not-a-tag` not valid for functions
    function g() public pure {
        /// @not-a-tag Nor in inline assembly.
        //~^ WARN: unexpected NatSpec tag `@not-a-tag` in inline assembly
        assembly {}
    }
}

/// @dev File-level variables accept `@dev`.
/// @custom:note And custom tags.
uint constant DEV = 1;

/// @dev A line without a tag continues the previous tag
/// instead of starting a notice.
uint constant CONTINUED = 1;

/**
 *
 */
uint constant DECORATION = 1;

/// @notice But not `@notice`.
/// @inheritdoc Base
//~^^ WARN: tag `@notice` not valid for file-level variables
//~^^ WARN: tag `@inheritdoc` not valid for file-level variables
uint constant NOTICE = 1;

// solc does not validate the documentation of structs, enums and UDVTs.

/// @title Struct
/// @param missing Not a field.
/// @return Nothing.
/// @inheritdoc Base
/// @not-a-tag Unknown.
struct S {
    uint a;
}

/// @param missing Not a value.
/// @return Nothing.
/// @inheritdoc Base
/// @not-a-tag Unknown.
enum E {
    A
}

/// @title UDVT
/// @author Someone
/// @return Nothing.
/// @not-a-tag Unknown.
type U is uint;
