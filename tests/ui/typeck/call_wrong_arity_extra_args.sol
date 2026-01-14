//@compile-flags: -Ztypeck

contract CallWrongArityExtraArgs {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function id(uint256 x) public pure returns (uint256) {
        return x;
    }

    function testExtraArgsAreStillChecked() public pure {
        add(1, 2, id(true)); //~ ERROR: wrong number of arguments: expected 2, found 3
        //~^ ERROR: mismatched types
    }
}
