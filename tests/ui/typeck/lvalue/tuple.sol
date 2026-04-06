//@compile-flags: -Ztypeck

contract Test {
    uint256 constant CONST = 1;
    uint256 state;

    function testTupleWithConstant() external {
        uint256 x = state;
        (CONST, state) = (x, x); //~ ERROR: cannot assign to a constant variable
    }

    function testTupleWithLiteral() external {
        uint256 x = state;
        (1, state) = (x, x); //~ ERROR: expression has to be an lvalue
        //~^ ERROR: mismatched types
    }
}
