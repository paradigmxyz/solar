//@compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-arithmetic
//@filecheck:

// At `-O none` a checked operation does not carry its own panic payload: its failure block calls
// one function that reverts with the panic code, so every check keeps its own block and location
// while the payload exists once.
contract CheckedArithmeticPanicNone {
    // CHECK-LABEL: fn @add{{[( ]}}
    // CHECK: icall @panic_error_0x11
    // CHECK-NOT: mstore 32, 17
    // CHECK-LABEL: fn @divide{{[( ]}}
    // CHECK: icall @panic_error_0x12
    // CHECK-LABEL: fn @panic_error_0x11{{[( ]}}
    // CHECK: mstore 32, 17
    // CHECK: revert 28, 36
    // CHECK-LABEL: fn @panic_error_0x12{{[( ]}}
    // CHECK: mstore 32, 18
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b + a;
    }

    function divide(uint256 a, uint256 b) public pure returns (uint256) {
        return a / b;
    }
}
