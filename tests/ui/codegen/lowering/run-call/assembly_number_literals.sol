//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: f => 1036

// Assembly accepts plain decimal and hexadecimal integer literals.
contract AssemblyNumberLiterals {
    // CHECK-LABEL: fn @f()
    // CHECK: ret 0x40c
    function f() public pure returns (uint256 r) {
        assembly {
            r := add(add(1000, 0x10), 20)
        }
    }
}
