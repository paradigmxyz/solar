//@compile-flags: -Ztypeck
// Tests for type-based function overload resolution (matching solc behavior)

contract OverloadResolution {
    // Basic overloads with different types
    function f(uint8 x) public pure returns (uint8) {
        return x;
    }

    function f(uint256 x) public pure returns (uint256) {
        return x;
    }

    // Test: 256 doesn't fit in uint8, resolves to f(uint256)
    function testLiteralOutOfRange() public pure {
        f(256); // OK - resolves to f(uint256)
    }

    // Test: 50 fits in both - ambiguous
    function testAmbiguous() public pure {
        f(50); //~ ERROR: ambiguous call
    }

    // Overloads with int vs uint
    function g(int256 x) public pure returns (int256) {
        return x;
    }

    function g(uint256 x) public pure returns (uint256) {
        return x;
    }

    // Test: negative literal resolves to int
    function testNegativeLiteral() public pure {
        g(-1); // OK - resolves to g(int256)
    }

    // Test: positive literal is ambiguous
    function testPositiveAmbiguous() public pure {
        g(50); //~ ERROR: ambiguous call
    }

    // Overloads with different arity
    function h(uint256 x) public pure returns (uint256) {
        return x;
    }

    function h(uint256 x, uint256 y) public pure returns (uint256) {
        return x + y;
    }

    // Test: arity determines resolution
    function testArityResolution() public pure {
        h(1); // OK - resolves to h(uint256)
        h(1, 2); // OK - resolves to h(uint256, uint256)
    }

    // Test: wrong arity
    function testWrongArity() public pure {
        h(1, 2, 3); //~ ERROR: no matching function found
    }
}
