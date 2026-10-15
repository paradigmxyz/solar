//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: A::slot => 1961591118816895610
//@ run-call: B::slot => 355

// Base slots take integer values that only the evaluation after type checking computes; see
// CONTRACT-001.

library L {
    uint64 constant BASE = 100;
}

contract A layout at uint64(uint256(keccak256("solar.a"))) {
    uint256 x;

    // CHECK-LABEL: fn @slot()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x1b38f8bac7a4627a
    function slot() external pure returns (uint256 s) {
        assembly {
            s := x.slot
        }
    }
}

contract B layout at L.BASE + type(uint8).max {
    uint256 y;

    // CHECK-LABEL: fn @slot()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 355
    function slot() external pure returns (uint256 s) {
        assembly {
            s := y.slot
        }
    }
}
