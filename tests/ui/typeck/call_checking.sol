//@ compile-flags: -Ztypeck
// Test function call type checking

contract CallChecking {
    event E(uint a, bytes32 b);
    error MyError(uint code, bytes32 message);

    function target(uint x, bytes32 y) public pure {}
    function noArgs() public pure returns (uint256) {
        return 42;
    }

    // === Correct positional arguments (no errors expected) ===
    function testPositional() public pure {
        target(1, "hi");
    }

    // === Named arguments not supported for function calls ===
    function testNamed() public pure {
        target({x: 1, y: "hi"}); //~ ERROR: named arguments are not supported
        target({y: "hi", x: 1}); //~ ERROR: named arguments are not supported
    }

    // === Wrong argument count ===
    function testWrongCount() public pure {
        target(1); //~ ERROR: wrong number of arguments
        target(1, "hi", 3); //~ ERROR: wrong number of arguments
        noArgs(1); //~ ERROR: wrong number of arguments
    }

    // === Wrong argument types ===
    function testWrongType() public pure {
        target("hi", 1);
        //~^ ERROR: mismatched types
        //~| ERROR: mismatched types
    }

    // === Named arguments not supported (error before duplicate check) ===
    function testDuplicateNamed() public pure {
        target({x: 1, x: 2, y: "hi"}); //~ ERROR: named arguments are not supported
    }

    // === Named arguments not supported (error before unknown check) ===
    function testUnknownNamed() public pure {
        target({x: 1, z: "hi"}); //~ ERROR: named arguments are not supported
    }

    // === Event emit - correct (no errors expected) ===
    function testEventCorrect() public {
        emit E(1, "hello");
        emit E({a: 1, b: "hello"});
    }

    // === Event emit - wrong count ===
    function testEventWrongCount() public {
        emit E(1); //~ ERROR: wrong number of arguments
    }

    // === Event emit - wrong type ===
    function testEventWrongType() public {
        emit E("hi", 1);
        //~^ ERROR: mismatched types
        //~| ERROR: mismatched types
    }

    // === Event emit - named argument errors ===
    function testEventNamedErrors() public {
        emit E({a: 1, a: 2, b: "hi"});
        //~^ ERROR: wrong number of arguments
        //~| ERROR: duplicate named argument
        emit E({a: 1, c: "hi"}); //~ ERROR: named argument `c` does not match
    }

    // === Event emit - mixed named/positional (not allowed) ===
    function testEventMixedArgs() public {
        // Solidity doesn't allow mixing positional and named args
        // This should error at parse time or as "expected }" 
        // emit E(1, {b: "hi"}); // Would be a parse error
    }

    // === Error/revert - correct (no errors expected) ===
    function testRevertCorrect() public pure {
        revert MyError(404, "not found");
        revert MyError({code: 404, message: "not found"});
    }

    // === Error/revert - wrong count ===
    function testRevertWrongCount() public pure {
        revert MyError(404); //~ ERROR: wrong number of arguments
    }

    // === Error/revert - wrong type ===
    function testRevertWrongType() public pure {
        revert MyError("hi", 404);
        //~^ ERROR: mismatched types
        //~| ERROR: mismatched types
    }

    // === Error/revert - named argument errors ===
    function testRevertNamedErrors() public pure {
        revert MyError({code: 1, code: 2, message: "hi"});
        //~^ ERROR: wrong number of arguments
        //~| ERROR: duplicate named argument
        revert MyError({code: 1, msg: "hi"}); //~ ERROR: named argument `msg` does not match
    }
}
