//@ codegen-matrix: standard
//@ run-call: i [9, 9, 9, 9], 1 => [3, 3, 4, 9]
//@ run-call: i [9, 9, 9, 9], 0 => [2, 3, 4, 9]

// The extra results of a multi-word call arrive through the multi-return buffer, and the
// backend binds them to stack words only while they are read as separate loads, so the stores
// of three of them to adjacent words stay word by word.

contract Test {
    function two(uint256 n) internal pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        if (n == 0) return (1, 2, 3, 4);
        (a, b, c, d) = two(n - 1);
        a += n;
        b += n;
    }
    function i(uint256[4] memory s, uint256 n) external pure returns (uint256[4] memory) {
        (, uint256 b, uint256 c, uint256 d) = two(n);
        s[0] = b;
        s[1] = c;
        s[2] = d;
        return s;
    }
}
