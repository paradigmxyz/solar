//@ revisions: none gas
//@[none] compile-flags: -Onone -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir-final
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// A pure function that only builds a struct runs once per call at `-Onone`, so its body and its
// `return` statement have hits for every call. Optimized builds build the struct in the caller.

// GAS-NOT: icall @set
// GAS: returndata 0, 32
contract C {
    struct S {
        int256 x;
        int256 y;
        int256 e;
    }

    function set(int256 x, int256 e) internal pure returns (S memory) {
        return S({x: x, y: 0, e: e});
    }

    // NONE-LABEL: fn @sum{{[( ]}}
    // NONE: icall @set
    // NONE: icall @set
    // NONE: icall @set
    function sum(int256 a, int256 b) external pure returns (int256) {
        S memory p = set(a, 1);
        S memory q = set(b, 2);
        S memory r = set(a, b);
        return p.x + q.x + r.e;
    }
}
