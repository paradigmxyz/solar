//@compile-flags: -Ztypeck
// Tests for named argument validation in function calls

contract NamedArgs {
    // Function with named parameters
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    // Test: valid named args
    function testValidNamedArgs() public pure {
        add({a: 10, b: 20}); // OK
        add({b: 20, a: 10}); // OK - order doesn't matter
    }

    // Test: duplicate named arg
    function testDuplicateArg() public pure {
        add({a: 10, a: 20}); //~ ERROR: duplicate argument `a`
        //~^ ERROR: missing argument `b`
    }

    // Test: unknown named arg
    function testUnknownArg() public pure {
        add({a: 10, c: 20}); //~ ERROR: unknown argument `c`
        //~^ ERROR: missing argument `b`
    }

    // Test: missing required arg
    function testMissingArg() public pure {
        add({a: 10}); //~ ERROR: missing argument `b`
    }

    // Test: wrong type with named arg
    function testWrongType() public pure {
        add({a: true, b: 100}); //~ ERROR: mismatched types
    }
}

// Test named args with overloaded functions
contract OverloadedNamedArgs {
    function process(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function process(
        uint256 a,
        uint256 b,
        uint256 c
    ) public pure returns (uint256) {
        return a + b + c;
    }

    function testOverloadWithNamed() public pure {
        process({a: 1, b: 2}); // OK - resolves to 2-arg version
        process({a: 1, b: 2, c: 3}); // OK - resolves to 3-arg version
    }
}
