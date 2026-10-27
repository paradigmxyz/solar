//@ compile-flags: -j8

contract C {
    function f1(S24 memory) public {}
    function f2(S23 memory) public {}
    function f3(S22 memory) public {}
    function f4(S21 memory) public {}
    function f5(S20 memory) public {}
    function f6(S19 memory) public {}
    function f7(S18 memory) public {}
    function f8(S17 memory) public {}
    function f9(S16 memory) public {}
    function f10(S15 memory) public {}
    function f11(S14 memory) public {}
    function f12(S13 memory) public {}
    function f13(S12 memory) public {}
    function f14(S11 memory) public {}
    function f15(S10 memory) public {}
    function f16(S9 memory) public {}
    function f17(S8 memory) public {}
    function f18(S7 memory) public {}
    function f19(S6 memory) public {}
    function f20(S5 memory) public {}
    function f21(S4 memory) public {}
    function f22(S3 memory) public {}
    function f23(S2 memory) public {}
    function f24(S1 memory) public {}
}

struct S1 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S2 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S3 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S4 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S5 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S6 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S7 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S8 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S9 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S10 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S11 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S12 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S13 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S14 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S15 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S16 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S17 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S18 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S19 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S20 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S21 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S22 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S23 { uint[0] a; } //~ ERROR: array length must be greater than zero
struct S24 { uint[0] a; } //~ ERROR: array length must be greater than zero
