//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/events/event_without_emit_deprecated.sol
// ported-from: test/libsolidity/syntaxTests/events/multiple_event_without_emit.sol

contract EventWithoutEmit {
    event MyEvent(uint a, bytes32 b);
    event EmptyEvent();

    function eventAsExpression() public {
        MyEvent(1, "hi"); //~ ERROR: event invocations have to be prefixed by `emit`
    }

    function eventInAssignment() public {
        uint x = MyEvent(1, "hi");
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched number of components
    }

    function eventAsArgument() public pure {
        this.takeBytes(EmptyEvent()); //~ ERROR: event invocations have to be prefixed by `emit`
        //~^ ERROR: mismatched types
    }

    function multipleEvents() external {
        emit MyEvent(0, "x");
        MyEvent(1, "y"); //~ ERROR: event invocations have to be prefixed by `emit`
    }

    function takeBytes(bytes memory) public pure {}
}
