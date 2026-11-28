//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir-final
//@[size] compile-flags: -Zdump=mir-final
//@[gas] filecheck:
//@[size] filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A `gasleft()` read holds back no other code, so the second store overwrites the first, as with
// solc's optimizer.
// CHECK-LABEL: fn @f()
// CHECK-NOT: sstore
// CHECK: = gas
// CHECK-NEXT: sstore 0, 2
// CHECK-NOT: sstore
// CHECK: returndata
contract GasleftBetweenStores {
    uint256 x;

    function f() external returns (uint256 g) {
        x = 1;
        g = gasleft();
        x = 2;
    }
}
