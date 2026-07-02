//@ compile-flags: -Ztypeck

contract C {
    function tupleExpression() public {
        uint256 x = (1,, true); //~ ERROR: tuple components cannot be empty
        //~^ ERROR: mismatched number of components
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function omittedIndex(uint256[] memory xs) public {
        uint256 x = xs[]; //~ ERROR: index expression cannot be omitted
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function memorySlice(uint256[] memory xs) public {
        uint256 x = xs[:]; //~ ERROR: can only slice dynamic calldata arrays
        //~^ ERROR: mismatched types
        uint8 y = 300; //~ ERROR: mismatched types
    }
}
