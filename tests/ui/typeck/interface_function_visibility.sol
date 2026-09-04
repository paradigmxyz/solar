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

// The restriction only applies to interfaces.
contract C {
    function f() public {}
    function g() internal {}
    function h() private {}
    function i() external {}
}
