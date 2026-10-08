//@ revisions: size byzantium
//@ compile-flags: -O size -Zdump=evm-ir-runtime
//@[byzantium] compile-flags: --evm-version byzantium
//@[size] filecheck: --check-prefix=SIZE
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

// Under `-O size`, signed and left-aligned immutables keep a narrow push up to
// 28 bytes and fall back to a full word above that. Without shift opcodes, only
// one-byte right-aligned immutables stay narrow.
contract ImmutableWidthsSize {
    int8 immutable a;
    int224 immutable b;
    int232 immutable c;
    bytes28 immutable d;
    bytes29 immutable e;
    uint8 immutable f;
    bytes1 immutable g;

    constructor(int8 a_, int224 b_, int232 c_, bytes28 d_, bytes29 e_, uint8 f_, bytes1 g_) {
        a = a_;
        b = b_;
        c = c_;
        d = d_;
        e = e_;
        f = f_;
        g = g_;
    }

    // SIZE-LABEL: @module ImmutableWidthsSize_runtime
    // SIZE: push_immutable 0, 1
    // SIZE: push_immutable 1, 28
    // SIZE: push_immutable 2, 32
    // SIZE: push_immutable 3, 28
    // SIZE: push_immutable 4, 32
    // SIZE: push_immutable 5, 1
    // SIZE: push_immutable 6, 1
    // BYZANTIUM-LABEL: @module ImmutableWidthsSize_runtime
    // BYZANTIUM: push_immutable 0, 1
    // BYZANTIUM: push_immutable 1, 32
    // BYZANTIUM: push_immutable 2, 32
    // BYZANTIUM: push_immutable 3, 32
    // BYZANTIUM: push_immutable 4, 32
    // BYZANTIUM: push_immutable 5, 1
    // BYZANTIUM: push_immutable 6, 32
    function read() external view returns (int8, int224, int232, bytes28, bytes29, uint8, bytes1) {
        return (a, b, c, d, e, f, g);
    }
}
