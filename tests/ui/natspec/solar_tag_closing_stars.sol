//@ check-pass
// A `**/` that closes the comment is not part of the arguments of a `@custom:solar-*` tag.

contract C {
    /** @custom:solar-cold **/
    function cold() internal pure returns (uint256) {
        return 1;
    }

    /** @custom:solar-inline always **/
    function inlined() internal pure returns (uint256) {
        return 2;
    }

    /**
     * @custom:solar-optimize gas
     **/
    function optimized() external pure returns (uint256) {
        return cold() + inlined();
    }
}
