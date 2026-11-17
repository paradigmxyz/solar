//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump only makes the tester run FileCheck; it reads the source map.

// DEBUG-006: a pure internal function whose only statement returns a struct constructor is
// lowered into each call. The function has no code of its own, and the code that builds the struct
// maps to the call, so neither `make` nor its `return` has an entry.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `function f`
// CHECK: {{[;"]}}[[#C+183]]{{[:;]}}
// `make(x, 1)`
// CHECK: {{[;"]}}[[#C+260]]{{[:;]}}
// `make(x, 2)`
// CHECK: {{[;"]}}[[#C+293]]{{[:;]}}
// `function make`, `return P(a, b);` and `P(a, b)`
// CHECK-NOT: {{[;"]}}[[#C+77]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+156]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+163]]{{[:;]}}

contract C {
    struct P {
        uint256 a;
        uint256 b;
    }

    function make(uint256 a, uint256 b) internal pure returns (P memory) {
        return P(a, b);
    }

    function f(uint256 x) external pure returns (uint256) {
        P memory p = make(x, 1);
        P memory q = make(x, 2);
        return p.a + q.b;
    }
}
