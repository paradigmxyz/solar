// `@custom:solar-optimize <gas|size>` picks what optimized builds of a contract
// or a library optimize its code for, so it must document code this compiler
// emits, and name exactly one objective.

/// @custom:solar-optimize size
contract Small {
    /// @custom:solar-optimize size
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract or a library
    function f() external pure returns (uint256) {
        return 1;
    }

    /// @custom:solar-optimize gas
    //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract or a library
    uint256 x;
}

/// @custom:solar-optimize gas
library Fast {
    function g() external pure returns (uint256) {
        return 2;
    }
}

/// @custom:solar-optimize size
//~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract or a library
interface I {
    function f() external;
}

/// @custom:solar-optimize size
//~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract or a library
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
