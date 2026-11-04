contract C {
    uint256 s;

    function f(uint256 x, uint256 y) external view returns (uint256) {
        unchecked {
            return (x * y + 3) + (x * y + 3) + s + s;
        }
    }

    function g(uint256 x) external view returns (uint256) {
        uint256 a = s + x;
        uint256 b = s + x;
        return a * b;
    }
}
