// ported-from: test/libsolidity/syntaxTests/controlFlow/leave_outside_function.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/leave_invalid.sol

contract C {
    function f() public pure {
        assembly {
            // Make sure this doesn't trigger the unimplemented assertion in the control flow builder.
            leave //~ ERROR: keyword `leave` can only be used inside a function
        }
    }

    function g() public pure {
        assembly {
            function inner() {
                leave
            }
            leave //~ ERROR: keyword `leave` can only be used inside a function
        }
    }
}
