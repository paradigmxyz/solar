//@ compile-flags: -Zcodegen -Zdump=evm-ir --evm-version byzantium
//@ filecheck:

contract ImmutableWidthsByzantium {
    uint8 immutable a;
    int16 immutable b;
    bytes3 immutable c;

    constructor(uint8 a_, int16 b_, bytes3 c_) {
        a = a_;
        b = b_;
        c = c_;
    }

    // CHECK-LABEL: @module runtime
    // CHECK: push_immutable 0, 32
    // CHECK: push_immutable 1, 32
    // CHECK: push_immutable 2, 32
    function read() external view returns (uint8, int16, bytes3) {
        return (a, b, c);
    }
}
