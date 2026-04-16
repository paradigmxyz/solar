//@ignore-host: windows
//@compile-flags: --emit=mir

contract Ternary {
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        return a > b ? a : b;
    }

    function clamp(uint256 x, uint256 lo, uint256 hi) public pure returns (uint256) {
        return x < lo ? lo : (x > hi ? hi : x);
    }

    function abs_diff(uint256 a, uint256 b) public pure returns (uint256) {
        return a >= b ? a - b : b - a;
    }
}
