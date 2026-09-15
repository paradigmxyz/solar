//@ compile-flags: -Ogas -Zdump=disasm-runtime
//@ run-call: 0xdeadbeef
//@ filecheck:

// CHECK-LABEL: data_fallthrough.sol:DataFallthrough (runtime)
// CHECK: STOP
// CHECK-NEXT: INVALID
contract DataFallthrough {
    event Seen(bytes data);

    fallback() external payable {
        emit Seen(
            hex"fe11111111111111111111111111111111111111111111111111111111111111"
            hex"1111111111111111111111111111111111111111111111111111111111111111"
            hex"1111111111111111111111111111111111111111111111111111111111111111"
            hex"1111111111111111111111111111111111111111111111111111111111111111"
            hex"1111111111111111111111111111111111111111111111111111111111111111"
        );
    }
}
