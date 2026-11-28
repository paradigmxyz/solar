// ported-from: test/libsolidity/syntaxTests/visibility/interface/function_default.sol
// ported-from: test/libsolidity/syntaxTests/visibility/interface/function_external.sol
// ported-from: test/libsolidity/syntaxTests/visibility/interface/function_internal.sol
// ported-from: test/libsolidity/syntaxTests/visibility/interface/function_private.sol
// ported-from: test/libsolidity/syntaxTests/visibility/interface/function_public.sol

interface I {
    function f() public; //~ ERROR: functions in interfaces must be declared `external`
    function g() internal; //~ ERROR: functions in interfaces must be declared `external`
    function h() private; //~ ERROR: functions in interfaces must be declared `external`
    function i() external;
}

interface J {
    // An omitted visibility is an error of its own, and defaults to `public`.
    function f(); //~ ERROR: no visibility specified
    //~^ ERROR: functions in interfaces must be declared `external`
}

// `fallback` and `receive` are already required to be `external`.
interface K {
    fallback() external;
    receive() external payable;
}

// Members that are not ordinary functions keep their own checks: solc rejects a constructor, a
// modifier, and a variable in an interface with a different error each, and the getter of a public
// variable is `external`, so none of them may report this one.
interface L {
    constructor() {}
    //~^ ERROR: functions in interfaces cannot have an implementation
    //~| ERROR: constructor cannot be defined in interfaces
    modifier m() { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    uint256 public x; //~ ERROR: variables cannot be declared in interfaces
}

// The restriction only applies to interfaces.
contract C {
    function f() public {}
    function g() internal {}
    function h() private {}
    function i() external {}
}
