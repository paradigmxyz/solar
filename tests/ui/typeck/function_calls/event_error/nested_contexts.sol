contract EventErrorNestedContexts {
    event MyEvent(uint a, bytes32 b);
    event EmptyEvent();
    error MyError(uint code, bytes32 message);
    error EmptyError();

    function nestedEventInEmitArg() public {
        emit MyEvent(EmptyEvent(), "x");
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }

    function nestedErrorInRevertArg() public pure {
        revert MyError(EmptyError(), "x");
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: mismatched types
    }

    function eventInBinaryOp() public {
        bool b = EmptyEvent() == EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: cannot apply builtin operator
    }

    function errorInBinaryOp() public pure {
        bool b = EmptyError() == EmptyError();
        //~^ ERROR: errors can only be used with revert statements
        //~| ERROR: errors can only be used with revert statements
        //~| ERROR: cannot apply builtin operator
    }

    function eventInArray() public {
        uint[2] memory arr = [EmptyEvent(), EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: cannot infer array element type
    }

    function eventInTernary() public {
        uint x = true ? EmptyEvent() : EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
        //~| ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
        //~| ERROR: mismatched number of components
    }

    struct S { uint x; }
    function eventInStruct() public {
        S memory s = S(EmptyEvent());
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }

    mapping(uint => uint) m;
    function eventInMappingKey() public {
        uint v = m[EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }

    function eventInArrayIndex() public {
        uint[] memory arr;
        uint v = arr[EmptyEvent()];
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }

    function multipleEventsInExpr() public {
        uint x = EmptyEvent() + EmptyEvent();
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: cannot apply builtin operator
    }
}
