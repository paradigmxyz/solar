//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::selector => 0xc1cd846c

// A library function can take a recursive storage struct, so its signature has ABI types and its
// selector folds. The value comes from solc.
library L {
    struct S {
        S[] children;
    }

    function f(S storage) public {}
}

contract C {
    bytes4 constant SELECTOR = L.f.selector;

    // CHECK-LABEL: fn @selector()
    // CHECK-NOT: icall
    // CHECK: ret
    function selector() external pure returns (bytes4) {
        return SELECTOR;
    }
}
