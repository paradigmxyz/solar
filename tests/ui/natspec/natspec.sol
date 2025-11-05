//@ compile-flags: --stop-after parsing

/// @title A Simple Contract
/// @author Test Author
contract BasicTags {
    /// @notice This function does something
    /// @dev Implementation details here
    function basicFunction() public {}
}

/// @notice This is a function with
/// very long documentation that spans
/// multiple lines
/// @dev The implementation is complex
/// and requires detailed explanation
/// across several lines
function multilineDoc() {}

// Block comment style with asterisks
/**
 * @title Block Comment Contract
 * @author Someone
 * @notice This uses block comment style
 * with asterisks on each line
 */
contract BlockStyle {
    /**
     * @notice Transfer tokens
     * @dev Internal implementation
     * @param to The recipient address
     * @param amount The amount to transfer
     * @return success Whether it succeeded
     */
    function transfer(address to, uint256 amount) public returns (bool success) {}
}

// Parameter and return tags
/// @param x First parameter
/// @param y Second parameter
/// @return sum The sum of x and y
/// @return product The product of x and y
function multipleParams(uint x, uint y) pure returns (uint sum, uint product) {}

// Inheritdoc tag
/// @inheritdoc IERC20
interface InheritExample {
    function balanceOf(address account) external view returns (uint256);
}

// Custom tags
/// @custom:security This has been audited
/// @custom:experimental This is experimental
contract CustomTags {
    /// @custom:fee 1% fee applies
    function customTaggedFunction() public {}
}

// Internal tags (special tags for tooling)
/// @src 1:2:3
/// @use-src 1:2:3
/// @ast-id 42
contract InternalTags {
    function internalTagged() public {
    /// @solidity memory-safe
        assembly {
           // do something
        }
    }
}

// Invalid tags (should emit errors)
/// @invalid This is not a valid tag
//~^ ERROR: invalid natspec tag '@invalid'
contract InvalidTag1 {}

// Invalid custom tag format
/// @customwrong Should use colon format
//~^ ERROR: invalid natspec tag '@customwrong', custom tags must use format '@custom:name'
contract InvalidCustom {}

// Yul context - should silently ignore unknown tags
contract YulContext {
    function testYul() public {
        assembly {
            /// @invalid This should not error in Yul
            function yulFunc() {}
        }
    }
}

/// @ author whitespace before tag is valid
/// @notice it is valid to add '@'s in the middle of a comment
///   * Solve: 2yr @ 5% == 1yr @ 0% + 1yr @ x => x = 10.00%
contract ReproLineComment {}

/**
* @ author whitespace before tag is valid
* @notice it is valid to add '@'s in the middle of a comment
*    Solve: 2yr @ 5% == 1yr @ 0% + 1yr @ x => x = 10.00%
**/
contract ReproBlockComment {}
