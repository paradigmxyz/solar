//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/revertStatement/error_used_elsewhere.sol

contract ErrorUsedElsewhere {
    error MyError(uint code, bytes32 message);
    error EmptyError();

    function errorAsExpression() public pure {
        MyError(404, "not found"); //~ ERROR: errors can only be used with revert statements
    }

    function errorInAssignment() public pure {
        uint x = MyError(404, "not found");
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched number of components
    }

    function errorAsArgument() public pure {
        this.takeBytes(EmptyError()); //~ ERROR: errors can only be used with revert statements
        //~^ ERROR: mismatched types
    }

    function takeBytes(bytes memory) public pure {}
}
