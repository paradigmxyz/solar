//@compile-flags: -Zspan-visitor

contract SpanVisitorTest {
    uint256 x;
    
    function foo() public {
        x = 42;
    }
}