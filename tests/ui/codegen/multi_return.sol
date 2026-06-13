//@ignore-host: windows
//@compile-flags: --emit=mir

contract MultiReturn {
    function div_mod(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (a / b, a % b);
    }

    function min_max(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        if (a < b) {
            return (a, b);
        }
        return (b, a);
    }

    function triple(uint256 x) public pure returns (uint256, uint256, uint256) {
        return (x, x + x, x + x + x);
    }
}
