//@compile-flags: -Zcodegen --emit=bin-runtime

contract StackArgRotation {
    uint256 public sink;

    function run(uint256 x) external returns (uint256 result) {
        uint256 a = x + 1;
        uint256 b = x ^ 7;
        result = mix(a, b);
        result += mix(x, 1);
        result += mix(x, 2);
        result += mix(x, 3);
    }

    function mix(uint256 a, uint256 b) internal returns (uint256) {
        unchecked {
            uint256 c = a + b;
            uint256 d = a * 3;
            uint256 e = d ^ b;
            sink ^= c;
            c += sink;
            d ^= c;
            e += d;
            return e;
        }
    }
}
