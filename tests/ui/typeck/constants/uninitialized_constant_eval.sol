// Evaluating a constant without an initializer reports nothing beyond the parser's error.

contract C {
    uint256 constant ZERO; //~ ERROR: constant variable must be initialized
    uint256[ZERO + 1] x;

    function f(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, ZERO);
    }
}
