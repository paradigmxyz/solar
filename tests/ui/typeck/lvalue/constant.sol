//@compile-flags: -Ztypeck
// TODO: `mismatched types` errors on integer literals are a current limitation of solar

contract Test {
    uint256 constant CONST = 1; //~ ERROR: mismatched types
    
    function test() external {
        CONST = 2; //~ ERROR: cannot assign to a constant variable
        //~^ ERROR: mismatched types
    }
}

uint256 constant FILE_CONST = 1; //~ ERROR: mismatched types

function fileLevel() {
    FILE_CONST = 2; //~ ERROR: cannot assign to a constant variable
    //~^ ERROR: mismatched types
}
