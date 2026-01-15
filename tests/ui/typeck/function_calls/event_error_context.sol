//@compile-flags: -Ztypeck

contract EventErrorContext {
    event MyEvent(uint a, bytes32 b);
    event EmptyEvent();
    error MyError(uint code, bytes32 message);
    error EmptyError();

    // === Valid usage ===
    function validEmit() public {
        emit MyEvent(1, "hi");
        emit EmptyEvent();
    }

    function validRevert() public pure {
        revert MyError(404, "not found");
        revert EmptyError();
    }

    // === Invalid: Event construction outside emit ===
    function eventAsExpression() public {
        MyEvent(1, "hi"); //~ ERROR: event invocations must be prefixed by "emit"
    }

    function eventInAssignment() public {
        uint x = MyEvent(1, "hi");
        //~^ ERROR: event invocations must be prefixed by "emit"
        //~| ERROR: mismatched number of components
    }

    function eventAsArgument() public pure {
        this.takeBytes(EmptyEvent()); //~ ERROR: event invocations must be prefixed by "emit"
        //~^ ERROR: mismatched types
    }

    function takeBytes(bytes memory) public pure {}

    // === Invalid: Error construction outside revert ===
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

}
