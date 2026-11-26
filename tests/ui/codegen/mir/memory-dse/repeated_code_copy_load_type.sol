//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A repeated copy of the same code word keeps each load's MIR type: an `i256` load never replaces
// a `ptr<memory>` load.
// CHECK-LABEL: fn @f(
// CHECK: codecopy 128,
// CHECK: mload 128
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
}
