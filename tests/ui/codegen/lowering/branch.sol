//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract Branch {
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        if (a > b) {
            return a;
        }
        return b;
    }

    function abs_diff(uint256 a, uint256 b) public pure returns (uint256) {
        if (a >= b) {
            return a - b;
        } else {
            return b - a;
        }
    }
}
