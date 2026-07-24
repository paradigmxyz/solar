//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract FunctionCall {
    function double(uint256 x) internal pure returns (uint256) {
        return x + x;
    }

    function quadruple(uint256 x) public pure returns (uint256) {
        return double(double(x));
    }

    function sum_then_double(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 s = a + b;
        return double(s);
    }
}
