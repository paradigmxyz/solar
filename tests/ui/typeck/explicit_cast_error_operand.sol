// Regression test for https://github.com/paradigmxyz/solar/issues/1760.

contract T {
    function f() external view returns (uint256) {
        return bytes(Missing.get()).length; //~ ERROR: unresolved symbol `Missing`
    }

    function g() external pure returns (uint256) {
        return string(Missing).length; //~ ERROR: unresolved symbol `Missing`
    }
}
