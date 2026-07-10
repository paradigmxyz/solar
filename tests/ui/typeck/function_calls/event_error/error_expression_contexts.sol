//@ compile-flags: -Ztypeck

contract C {
    error MyError(uint256 code, bytes32 message);
    error EmptyError();

    function errorInAssignment() public pure {
        uint256 x = MyError(404, "not found");
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched number of components
        x;
    }

    function errorAsArgument() public pure {
        this.takeBytes(EmptyError()); //~ ERROR: errors can only be used with revert statements
        //~^ ERROR: mismatched types
    }

    function takeBytes(bytes memory) public pure {}
}
