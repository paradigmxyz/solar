//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/events/event_library_function.sol

library L {
    function f() public {
        int256 x = 1;
        x;
    }
}
contract C {
    event Test(function() external indexed);

    function g() public {
        Test(L.f);
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }
}
contract D {
    event Test(function() external);

    function f() public {
        Test(L.f);
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }
}
contract E {
    event Test(function() external indexed);
    using L for D;

    function k() public {
        Test(D.f);
        //~^ ERROR: event invocations have to be prefixed by `emit`
        //~| ERROR: mismatched types
    }
}
