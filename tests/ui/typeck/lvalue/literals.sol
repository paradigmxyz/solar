//@compile-flags: -Ztypeck
// TODO: `mismatched types` errors on integer literals are a current limitation of solar

contract Test {
    uint256 state;
    bool boolState;
    
    function testInt() external {
        1 = state; //~ ERROR: expression has to be an lvalue
        //~^ ERROR: mismatched types
    }
    
    function testBool() external {
        true = boolState; //~ ERROR: expression has to be an lvalue
    }
}
