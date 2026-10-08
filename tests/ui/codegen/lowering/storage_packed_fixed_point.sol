//@ run-call: check => 1

// Fixed-point state variables take `M / 8` bytes and pack like other value
// types; bare `fixed` is `fixed128x18`.
contract C {
    fixed64x10 a; // slot 0, offset 0
    uint64 b; // slot 0, offset 8
    ufixed8x1 c; // slot 0, offset 16
    uint8 d; // slot 0, offset 17
    fixed e; // slot 1, offset 0
    uint128 f; // slot 1, offset 16

    function check() external returns (uint256) {
        b = 1;
        d = 2;
        f = 3;
        uint256 s0;
        uint256 s1;
        assembly {
            s0 := sload(0)
            s1 := sload(1)
        }
        require(s0 == (1 << 64) | (2 << 136), "slot0");
        require(s1 == 3 << 128, "slot1");
        return 1;
    }
}
