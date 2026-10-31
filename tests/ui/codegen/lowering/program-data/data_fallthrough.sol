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
            hex"fed9ff4e514f14b91fb55e834d7f69d664b71f8df58ab5a0016793d3b13180a5"
            hex"fd0a150d03d547c71a6438596f7823f68e77f0087f5a03216126712c83aa56a0"
            hex"bf8c274b4fad1e61ecf62d776b355078992ef6613311de3b7240ee3e709af7ed"
            hex"f515a6fb4c7226d4ff711f1a3a495f7468270cc591d28cbb78f7e5df85bb0c6a"
            hex"1a81720686b5af343e9407eb480687bd71b38d76711c308d172749e1e8e45eee"
        );
    }
}
