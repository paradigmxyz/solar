contract RequireCustomError {
    error EmptyError();
    error MyError(uint code, string message);

    function valid(bool condition) public pure {
        require(condition, EmptyError());
        require(condition, MyError(1, "failed"));
        require(condition, MyError({code: 1, message: "failed"}));
    }

    function invalidErrorArgs(bool condition) public pure {
        require(condition, MyError(1)); //~ ERROR: wrong argument count
    }

    function nestedErrorStillRejected(bool condition) public pure {
        require(condition, MyError(EmptyError(), "failed"));
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched types
    }

    function nonErrorRequireArgument(bool condition) public pure {
        require(condition, message(EmptyError()));
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched types
    }

    function message(uint) internal pure returns (string memory) {
        return "failed";
    }

    function errorOutsideRevertOrRequire() public pure {
        EmptyError(); //~ ERROR: errors can only be used with revert statements
    }
}
