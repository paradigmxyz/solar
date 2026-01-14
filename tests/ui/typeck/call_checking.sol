//@compile-flags: -Ztypeck
// Test function call type checking

contract CallChecking {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
    
    function noArgs() public pure returns (uint256) {
        return 42;
    }
    
    // Wrong argument count
    function wrongArgCount() public pure {
        add(1); //~ ERROR: wrong number of arguments
        add(1, 2, 3); //~ ERROR: wrong number of arguments
        noArgs(1); //~ ERROR: wrong number of arguments
    }
    
    // Wrong argument types
    function wrongArgTypes() public pure {
        add("hello", 2); //~ ERROR: mismatched types
    }
}
