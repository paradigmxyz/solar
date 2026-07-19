//@compile-flags: -Zcodegen --emit=mir

contract InternalCallFallbacks {
    function recurse(uint256 x) public returns (uint256) {
        return a(x);
    }

    function a(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : b(x - 1);
    }

    function b(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : a(x - 1);
    }

    function multi(uint256 x) public pure returns (uint256, uint256) {
        return pair(x);
    }

    function pair(uint256 x) internal pure returns (uint256, uint256) {
        return (x, x + 1);
    }
}
