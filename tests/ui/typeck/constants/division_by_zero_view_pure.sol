// Like solc's `StaticAnalyzer`, the division by zero check runs before the view and pure checks,
// whose errors would hide it.
uint256 constant ZERO = 0;

contract C {
    uint256 s;

    function f() public pure returns (uint256) {
        return s + 1 / ZERO; //~ ERROR: division by zero
    }
}
