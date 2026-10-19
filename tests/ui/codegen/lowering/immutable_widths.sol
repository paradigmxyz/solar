//@ revisions: gas none
//@ compile-flags: -Zdump=evm-ir-runtime
//@[none] compile-flags: -O none
//@[gas] filecheck:
//@[none] filecheck: --check-prefix=NONE

type Tiny is uint16;

// Immutables push their source width without optimization too; only an
// `address` that inline assembly may have written keeps a full word.
contract ImmutableWidths {
    uint8 immutable a;
    int16 immutable b;
    bytes3 immutable c;
    address immutable d;
    uint immutable e;
    Tiny immutable f;
    bool immutable g;
    address immutable h;

    constructor(uint8 a_, int16 b_, bytes3 c_, address d_, uint e_, Tiny f_, bool g_) {
        a = a_;
        b = b_;
        c = c_;
        d = d_;
        e = e_;
        f = f_;
        g = g_;
        address raw;
        assembly {
            raw := not(0)
        }
        h = raw;
    }

    // CHECK-LABEL: @module ImmutableWidths_runtime
    // CHECK: push_immutable 0, 1
    // CHECK-NEXT: push_immutable 1, 32
    // CHECK-NEXT: push 0xffff
    // CHECK-NEXT: and
    // CHECK-NEXT: push_immutable 2, 32
    // CHECK-NEXT: push_immutable 3, 20
    // CHECK-NEXT: push_immutable 4, 32
    // CHECK-NEXT: push_immutable 5, 2
    // CHECK-NEXT: push_immutable 6, 1
    // NONE-LABEL: @module ImmutableWidths_runtime
    // NONE-DAG: push_immutable 0, 1
    // NONE-DAG: push_immutable 3, 20
    // NONE-DAG: push_immutable 5, 2
    // NONE-DAG: push_immutable 6, 1
    // NONE-DAG: push_immutable 7, 32
    function read() external view returns (uint8, int16, bytes3, address, uint, Tiny, bool) {
        return (a, b, c, d, e, f, g);
    }

    // CHECK: push_immutable 7, 32
    function readRaw() external view returns (address) {
        return h;
    }
}
