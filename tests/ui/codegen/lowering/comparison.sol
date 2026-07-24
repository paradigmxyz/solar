//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract Comparison {
    function eq(uint256 a, uint256 b) public pure returns (bool) {
        return a == b;
    }

    function lt(uint256 a, uint256 b) public pure returns (bool) {
        return a < b;
    }

    function is_zero(uint256 a) public pure returns (bool) {
        return a == 0;
    }
}
