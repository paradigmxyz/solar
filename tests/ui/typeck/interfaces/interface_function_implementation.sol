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

// A Yul function inside the body is an item of its own, but only the Solidity function is
// reported.
interface L {
    function g() external pure { //~ ERROR: functions in interfaces cannot have an implementation
        assembly {
            function inner() -> r { r := 1 }
            pop(inner())
        }
    }
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
