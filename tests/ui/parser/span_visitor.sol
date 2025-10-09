//@compile-flags: -Zspan-visitor

/// @title SpanVisitorTest
/// @author solar contributors
///
/// @notice a contract to test the span visitor
contract SpanVisitorTest {
    /// @notice this is a variable
    uint256 x;

    /**
     * @notice this is a function
     * @param bar has one parameter
     * @return always returns 42
     *
     * @dev it is useless
     **/
    function foo(uint256 bar) public returns (uint256) {
        x = 42;
        return x;
    }
}
