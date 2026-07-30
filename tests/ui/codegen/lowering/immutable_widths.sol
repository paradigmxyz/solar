//@compile-flags: -Zcodegen -Zdump=evm-ir
//@filecheck:

type Tiny is uint16;

contract ImmutableWidths {
    uint8 immutable a;
    int16 immutable b;
    bytes3 immutable c;
    address immutable d;
    uint immutable e;
    Tiny immutable f;

    constructor(uint8 a_, int16 b_, bytes3 c_, address d_, uint e_, Tiny f_) {
        a = a_;
        b = b_;
        c = c_;
        d = d_;
        e = e_;
        f = f_;
    }

    // CHECK-LABEL: @module runtime
    // CHECK: push_immutable 0, 1
    // CHECK-NEXT: push_immutable 1, 32
    // CHECK-NEXT: push_immutable 2, 32
    // CHECK-NEXT: push_immutable 3, 20
    // CHECK-NEXT: push_immutable 4, 32
    // CHECK-NEXT: push_immutable 5, 2
    function read() external view returns (uint8, int16, bytes3, address, uint, Tiny) {
        return (a, b, c, d, e, f);
    }
}
