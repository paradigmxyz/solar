// `@custom:solar-optimize <gas|size>` picks what the code of a contract, a library or a function
// is optimized for, so it must document code this compiler emits, and name exactly one objective.

/// @custom:solar-optimize size
contract Small {
    /// @custom:solar-optimize gas
    function f() external pure returns (uint256) {
        return 1;
    }

    /// @custom:solar-optimize size
    function g() internal pure returns (uint256) {
        return 2;
    }

    /// @custom:solar-optimize size
    function h() private pure returns (uint256) {
        return 3;
    }

    /// @custom:solar-optimize gas
    function i() public pure returns (uint256) {
        return g() + h();
    }

    /// @custom:solar-optimize gas
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
    uint256 x;

    /// @custom:solar-optimize gas
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
    constructor() {}

    /// @custom:solar-optimize gas
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
    modifier m() {
        _;
    }

    /// @custom:solar-optimize speed
    //~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
    function unknown() external pure {}

    /// @custom:solar-optimize size
    /// @custom:solar-optimize gas
    //~^ ERROR: duplicate `@custom:solar-optimize` tag
    function twice() external pure {}
}

/// @custom:solar-optimize gas
library Fast {
    function g() external pure returns (uint256) {
        return 2;
    }
}

/// @custom:solar-optimize size
//~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
interface I {
    /// @custom:solar-optimize size
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
    function f() external;
}

/// @custom:solar-optimize size
//~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract, a library, or a function with a body
abstract contract Base {}

/// @custom:solar-optimize speed
//~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
contract Unknown {}

/// @custom:solar-optimize
//~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
contract Missing {}

/// @custom:solar-optimize size gas
//~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
contract Both {}

/// @custom:solar-optimize size
/// @custom:solar-optimize gas
//~^ ERROR: duplicate `@custom:solar-optimize` tag
contract Twice {}
