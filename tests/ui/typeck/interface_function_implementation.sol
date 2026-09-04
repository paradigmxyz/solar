// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/416_interface_function_bodies.sol

interface I {
    function f() external pure { //~ ERROR: functions in interfaces cannot have an implementation
    }
}

// An empty body is an implementation too, for every kind of function.
interface J {
    function f() external {} //~ ERROR: functions in interfaces cannot have an implementation
    fallback() external {} //~ ERROR: functions in interfaces cannot have an implementation
    receive() external payable {} //~ ERROR: functions in interfaces cannot have an implementation
}

// Declarations without a body are what an interface is for.
interface K {
    function f() external pure returns (uint256);
    fallback() external;
    receive() external payable;
}

// The restriction only applies to interfaces.
abstract contract C {
    function f() external pure {}
    function g() external pure virtual;
}
