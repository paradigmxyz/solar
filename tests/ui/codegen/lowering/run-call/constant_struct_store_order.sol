//@ codegen-matrix: standard
//@ run-call-fail: f() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A constant whose value is computed at runtime can revert, so the struct is built before the
// storage index is checked, like solc does.
contract ConstantStructStoreOrder {
    struct S {
        uint8 a;
        uint8 b;
    }

    S[] s;
    uint8 constant A = 255;
    uint8 constant B = A + 1;

    function f() external {
        s[0] = S(B, 1);
    }
}
