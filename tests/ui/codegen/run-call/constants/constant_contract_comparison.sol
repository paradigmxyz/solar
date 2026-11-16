//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: same => true, false

// Constant evaluation compares contract values by address.
interface I {
    function f() external;
}

contract ConstantContractComparison {
    bool constant SAME = I(address(1)) == I(address(1));
    bool constant LESS = I(address(2)) < I(address(1));

    // CHECK-LABEL: fn @same()
    // CHECK-NOT: eq
    // CHECK: ret
    function same() external pure returns (bool, bool) {
        return (SAME, LESS);
    }
}
