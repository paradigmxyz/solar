//@compile-flags: -Ztypeck

// Tests for event/error invocation context validation.
// Based on solc tests:
// - syntaxTests/events/event_without_emit_deprecated.sol
// - syntaxTests/events/multiple_event_without_emit.sol
// - syntaxTests/emit/emit_non_event.sol
// - syntaxTests/revertStatement/error_used_elsewhere.sol
// - syntaxTests/revertStatement/revert_event.sol

contract EventErrorContext {
    event MyEvent(uint a, bytes32 b);
    event EmptyEvent();
    error MyError(uint code, bytes32 message);
    error EmptyError();

    function() Test;

    // === Valid usage ===
    function validEmit() public {
        emit MyEvent(1, "hi");
        emit EmptyEvent();
    }

    function validRevert() public pure {
        revert MyError(404, "not found");
        revert EmptyError();
    }

    // === Event invocations outside emit (solc error 3132) ===
    function eventAsExpression() public {
        MyEvent(1, "hi"); //~ ERROR: event invocations have to be prefixed by "emit"
    }

    function eventInAssignment() public {
        uint x = MyEvent(1, "hi");
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched number of components
    }

    function eventAsArgument() public pure {
        this.takeBytes(EmptyEvent()); //~ ERROR: event invocations have to be prefixed by "emit"
        //~^ ERROR: mismatched types
    }

    // Solc test: multiple_event_without_emit.sol
    function multipleEvents() external {
        emit MyEvent(0, "x");
        // Second invocation without emit should still error.
        MyEvent(1, "y"); //~ ERROR: event invocations have to be prefixed by "emit"
    }

    function takeBytes(bytes memory) public pure {}

    // === Error invocations outside revert (solc error 7757) ===
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

    // === Non-event in emit statement (solc error 9292) ===
    function emitNonEvent() public {
        emit Test(); //~ ERROR: expression has to be an event invocation
    }

    // === Non-error in revert statement (solc error 1885) ===
    function revertEvent() public pure {
        revert EmptyEvent(); //~ ERROR: event invocations have to be prefixed by "emit"
        //~^ ERROR: expression has to be an error
    }

    // === Nested event/error invocations in arguments should still error ===
    function nestedEventInEmitArg() public {
        emit MyEvent(EmptyEvent(), "x");
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
    }

    function nestedErrorInRevertArg() public pure {
        revert MyError(EmptyError(), "x");
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched types
    }

    // === Event/error in various invalid contexts ===

    // In binary operations
    function eventInBinaryOp() public {
        bool b = EmptyEvent() == EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: cannot apply builtin operator
    }

    function errorInBinaryOp() public pure {
        bool b = EmptyError() == EmptyError();
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: errors can only be used with revert statements
        //~| ERROR: cannot apply builtin operator
    }

    // In array literal
    function eventInArray() public {
        uint[2] memory arr = [EmptyEvent(), EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: cannot infer array element type
    }

    // In ternary operator
    function eventInTernary() public {
        uint x = true ? EmptyEvent() : EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
        //~| ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
        //~| ERROR: mismatched number of components
    }

    // In struct constructor
    struct S { uint x; }
    function eventInStruct() public {
        S memory s = S(EmptyEvent());
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
    }

    // In mapping access
    mapping(uint => uint) m;
    function eventInMappingKey() public {
        uint v = m[EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
    }

    // In array index
    function eventInArrayIndex() public {
        uint[] memory arr;
        uint v = arr[EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: mismatched types
    }

    // Multiple events/errors in same expression
    function multipleEventsInExpr() public {
        uint x = EmptyEvent() + EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: event invocations have to be prefixed by "emit"
        //~| ERROR: cannot apply builtin operator
    }

    // TODO: require(condition, MyError(...)) should be allowed but is not yet implemented.
    // See: syntaxTests/errors/require_custom.sol
}
