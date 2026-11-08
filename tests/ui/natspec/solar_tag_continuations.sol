/**
 * @custom:solar-optimize size
 *
 * @notice A decorated contract.
 */
contract Sized {}

/**
 * @custom:solar-safe memory
 * arithmetic
 *
 */
contract Decorated {
    function run(uint256 value) external pure returns (uint256) { return value + 1; }
}

/// @custom:solar-safe memory
/// arithmetic
contract Continued {
    function run(uint256 value) external pure returns (uint256) {
        unchecked { return value + 1; } //~ ERROR: `Continued` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }
}

/// @custom:solar-safe memory
/// imaginary
//~^^ ERROR: `@custom:solar-safe` has no property `imaginary`
contract UnknownProperty {}

/// @custom:solar-optimize
/// size
contract ContinuedObjective {}
