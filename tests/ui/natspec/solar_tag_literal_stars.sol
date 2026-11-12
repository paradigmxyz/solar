// A `*` is part of the arguments of a `@custom:solar-*` tag, unless it is the one of a `**/` that
// closes a block comment.

contract C {
    /// @custom:solar-optimize gas*
    //~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
    function lineComment() external pure returns (uint256) {
        return 1;
    }

    /// @custom:solar-cold *
    //~^ ERROR: `@custom:solar-cold` takes no argument
    function literalArgument() internal pure returns (uint256) {
        return 2;
    }

    /** @custom:solar-optimize gas* **/
    //~^ ERROR: `@custom:solar-optimize` must name one objective, `gas` or `size`
    function blockComment() external pure returns (uint256) {
        return literalArgument();
    }
}
