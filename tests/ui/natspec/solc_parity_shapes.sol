// NatSpec shapes solc accepts and solar must too:
// - `@param -` documents an unnamed parameter (names parse as identifier
//   prefixes, so `-` is the empty name, valid while unnamed variables exist).
// - `@param` may reference return variable names.
// - Duplicate `@param` tags are not diagnosed.
// - `@ param` (whitespace after `@`) is continuation text, not a tag.
// - `* * @return` still starts a tag despite the doubled decoration.
// - A bare `@return name` at the end of a line keeps the name intact.

contract C {
    /**
     * @dev Ratifies that the parties have received the correct items.
     *
     * @param context         The context of the order.
     * @ param orderHashes     The order hashes, unused here.
     * @ param contractNonce   The contract nonce, unused here.
     *
     * @return ratifyOrderMagicValue The magic value to indicate things are OK.
     */
    function ratifyOrder(bytes calldata context, bytes32[] calldata, uint256)
        external
        pure
        returns (bytes4 ratifyOrderMagicValue)
    {
        context;
        return 0x12345678;
    }

    /**
     * @param -               caller, unused here.
     * @param -               fulfiller, unused here.
     * @param named           The named one.
     * @param named           Documented twice, accepted.
     * @param result          Return names are valid param targets.
     */
    function previewOrder(address, address, uint256 named)
        public
        pure
        returns (uint256 result)
    {
        return named;
    }

    /**
     * @param a first param.
     * * @return x The first return.
     * @return y The second return.
     */
    function doubled(uint256 a) public pure returns (uint256 x, uint256 y) {
        return (a, a);
    }

    /// @return fulfillments
    function bare(uint256 a) public pure returns (uint256 fulfillments) {
        return a;
    }

    /**
     * @param nope Not a parameter anywhere.
     */
    function bad(uint256 a) public pure returns (uint256) {
        //~^^^ ERROR: tag `@param` references non-existent parameter 'nope'
        return a;
    }
}
