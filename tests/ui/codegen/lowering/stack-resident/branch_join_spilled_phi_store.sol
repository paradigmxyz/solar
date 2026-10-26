//@ codegen-matrix: standard
//@ compile-flags: --allow=2018
//@ run-call: g 0 => 5689
//@ run-call: g 1 => 5691
//@ run-call: g 4 => 5697
//@ run-call: g 5 => 3007
//@ run-call: g 7 => 319

// The join after the `if` has a spilled phi `r`. Its edge from the branch already matches the
// join layout, but it must still store the phi input 5 into the slot of `r`.
contract C {
    mapping(uint256 => uint256) data;

    function h(uint256 a) internal returns (uint256) {
        uint256 r = 5;
        if (a > 5) {
            r = data[a];
        }
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
        return ((s ^ t) % 1000003) + a;
    }

    function g(uint256 a) external returns (uint256) {
        return h(a) + h(a + 1);
    }
}
