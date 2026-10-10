//@ codegen-matrix: standard
//@ run-call: g 0 => 1
//@ run-call: g 1 => 708
//@ run-call: g 2 => 332968
//@ run-call: g 3 => 215701
//@ run-call: g 5 => 478298

// `r`, the result of the recursive call, spills. The caller keeps the spilled words it still
// needs across a call that may reenter it and stores them back afterwards; the call's own
// result must not be among them, or the stale word overwrites the result just stored.
contract C {
    function f(uint256 n) internal returns (uint256) {
        if (n == 0) return 1;
        uint256 r = f(n - 1);
        uint256 x1 = r * 3 + 1;
        uint256 x2 = r * 5 + 2;
        uint256 x3 = r * 7 + 3;
        uint256 x4 = r * 11 + 4;
        uint256 x5 = r * 13 + 5;
        uint256 x6 = r * 17 + 6;
        uint256 x7 = r * 19 + 7;
        uint256 x8 = r * 23 + 8;
        uint256 x9 = r * 29 + 9;
        uint256 x10 = r * 31 + 10;
        uint256 x11 = r * 37 + 11;
        uint256 x12 = r * 41 + 12;
        uint256 x13 = r * 43 + 13;
        uint256 x14 = r * 47 + 14;
        uint256 x15 = r * 53 + 15;
        uint256 x16 = r * 59 + 16;
        uint256 x17 = r * 61 + 17;
        uint256 s = x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10 + x11 + x12 + x13 + x14 + x15 + x16 + x17;
        uint256 t = x17 ^ x16 ^ x15 ^ x14 ^ x13 ^ x12 ^ x11 ^ x10 ^ x9 ^ x8 ^ x7 ^ x6 ^ x5 ^ x4 ^ x3 ^ x2 ^ x1;
        return (s ^ t) % 1000003;
    }

    function g(uint256 n) external returns (uint256) {
        return f(n);
    }
}
