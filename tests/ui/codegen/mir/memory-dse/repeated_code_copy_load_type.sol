//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A repeated copy of the same code word reuses only word loads: an `i256` load never replaces a
// `ptr<memory>` load, and a `ptr<memory>` load is never stored back as a word.
// CHECK-LABEL: fn @f(
// CHECK: codecopy 128,
// CHECK: mload 128
// CHECK: ret
//
// CHECK-LABEL: fn @g(
// CHECK: ret
contract Test {
    struct S {
        bytes b;
    }

    function f() external pure returns (uint256 raw, bytes memory p) {
        S memory s;
        assembly {
            let src := sub(codesize(), 32)
            codecopy(0x80, src, 32)
            raw := mload(0x80)
            codecopy(s, src, 32)
        }
        p = s.b;
    }

    function g() external pure returns (bytes memory p, bytes memory q, uint256 raw) {
        S memory s;
        S memory t;
        uint256 src;
        assembly {
            src := sub(codesize(), 32)
        }
        assembly {
            codecopy(s, src, 32)
        }
        p = s.b;
        assembly {
            codecopy(t, src, 32)
        }
        q = t.b;
        assembly {
            raw := mload(t)
        }
    }
}
