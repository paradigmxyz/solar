//@ compile-flags: -Osize
//@ run-call: run 1019 => 21796157974083048550319244236929488537086114760591164995662604048548353814576
//@ run-call-fail: run 1020

// Every activation leaves one word on the stack, so the call overflows it just above 1019 nested
// activations. The continuation after the recursive call has no known entry depth, and compiling
// it must not move that boundary.
contract DeepRecursion {
    function run(uint256 n) external pure returns (uint256) {
        return depth(n);
    }

    function depth(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return depth(n - 1) ^ 0x3030303030303030303030303030303030303030303030303030303030303030;
    }
}
