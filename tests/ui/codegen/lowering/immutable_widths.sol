//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir
//@filecheck: --check-prefix=WIDTH

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

    function read() external view returns (uint8, int16, bytes3, address, uint, Tiny) {
        return (a, b, c, d, e, f);
    }
}

// WIDTH: push_immutable 0, 1
// WIDTH: push_immutable 1, 2
// WIDTH-NEXT: push 1
// WIDTH-NEXT: signextend
// WIDTH: push_immutable 2, 3
// WIDTH-NEXT: push 232
// WIDTH-NEXT: shl
// WIDTH: push_immutable 3, 20
// WIDTH: push_immutable 4, 32
// WIDTH: push_immutable 5, 2
