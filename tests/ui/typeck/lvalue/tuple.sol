//@compile-flags: -Ztypeck
// TODO: `mismatched types` errors on integer literals are a current limitation of solar

contract Test {
    uint256 constant CONST = 1; //~ ERROR: mismatched types
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
