//@ compile-flags: -O size -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Size-mode selector dispatch for contracts whose functions are empty. Every
// empty function ends in the same STOP, which block layout can place near the
// dispatch so its label fits one byte.

contract AllTerminal {
    // Tail merging and the short STOP label make a binary search with eight
    // leaves of ten smaller than a linear scan.
    // CHECK-LABEL: @module AllTerminal_runtime
    // CHECK-COUNT-7: {{^  gt$}}
    // CHECK-NOT: {{^  gt$}}
    function f0() external {}
    function f1() external {}
    function f2() external {}
    function f3() external {}
    function f4() external {}
    function f5() external {}
    function f6() external {}
    function f7() external {}
    function f8() external {}
    function f9() external {}
    function f10() external {}
    function f11() external {}
    function f12() external {}
    function f13() external {}
    function f14() external {}
    function f15() external {}
    function f16() external {}
    function f17() external {}
    function f18() external {}
    function f19() external {}
    function f20() external {}
    function f21() external {}
    function f22() external {}
    function f23() external {}
    function f24() external {}
    function f25() external {}
    function f26() external {}
    function f27() external {}
    function f28() external {}
    function f29() external {}
    function f30() external {}
    function f31() external {}
    function f32() external {}
    function f33() external {}
    function f34() external {}
    function f35() external {}
    function f36() external {}
    function f37() external {}
    function f38() external {}
    function f39() external {}
    function f40() external {}
    function f41() external {}
    function f42() external {}
    function f43() external {}
    function f44() external {}
    function f45() external {}
    function f46() external {}
    function f47() external {}
    function f48() external {}
    function f49() external {}
    function f50() external {}
    function f51() external {}
    function f52() external {}
    function f53() external {}
    function f54() external {}
    function f55() external {}
    function f56() external {}
    function f57() external {}
    function f58() external {}
    function f59() external {}
    function f60() external {}
    function f61() external {}
    function f62() external {}
    function f63() external {}
    function f64() external {}
    function f65() external {}
    function f66() external {}
    function f67() external {}
    function f68() external {}
    function f69() external {}
    function f70() external {}
    function f71() external {}
    function f72() external {}
    function f73() external {}
    function f74() external {}
    function f75() external {}
    function f76() external {}
    function f77() external {}
    function f78() external {}
    function f79() external {}
}

contract PartialTerminal {
    // With one function body after the dispatch, the split jumps of a binary
    // search reach past byte 255 and need two-byte labels. That cost keeps the
    // linear scan.
    // CHECK-LABEL: @module PartialTerminal_runtime
    // CHECK-NOT: {{^  gt$}}
    function f0() external view returns (uint256) {
        return block.number;
    }

    function f1() external {}
    function f2() external {}
    function f3() external {}
    function f4() external {}
    function f5() external {}
    function f6() external {}
    function f7() external {}
    function f8() external {}
    function f9() external {}
    function f10() external {}
    function f11() external {}
    function f12() external {}
    function f13() external {}
    function f14() external {}
    function f15() external {}
    function f16() external {}
    function f17() external {}
    function f18() external {}
    function f19() external {}
    function f20() external {}
    function f21() external {}
    function f22() external {}
    function f23() external {}
    function f24() external {}
    function f25() external {}
    function f26() external {}
    function f27() external {}
    function f28() external {}
    function f29() external {}
    function f30() external {}
    function f31() external {}
    function f32() external {}
    function f33() external {}
    function f34() external {}
    function f35() external {}
    function f36() external {}
    function f37() external {}
    function f38() external {}
    function f39() external {}
    function f40() external {}
    function f41() external {}
    function f42() external {}
    function f43() external {}
    function f44() external {}
    function f45() external {}
    function f46() external {}
    function f47() external {}
    function f48() external {}
    function f49() external {}
    function f50() external {}
    function f51() external {}
    function f52() external {}
    function f53() external {}
    function f54() external {}
    function f55() external {}
    function f56() external {}
    function f57() external {}
    function f58() external {}
    function f59() external {}
    function f60() external {}
    function f61() external {}
    function f62() external {}
    function f63() external {}
    function f64() external {}
    function f65() external {}
    function f66() external {}
    function f67() external {}
    function f68() external {}
    function f69() external {}
    function f70() external {}
    function f71() external {}
    function f72() external {}
    function f73() external {}
    function f74() external {}
    function f75() external {}
    function f76() external {}
    function f77() external {}
    function f78() external {}
    function f79() external {}
}

contract OutlinedFallback {
    // The fallback body sits outside the dispatch, so a binary search gets no
    // credit for short STOP labels and the linear scan stays smaller.
    // CHECK-LABEL: @module OutlinedFallback_runtime
    // CHECK-NOT: {{^  gt$}}
    function f0() external {}
    function f1() external {}
    function f2() external {}
    function f3() external {}
    function f4() external {}
    function f5() external {}
    function f6() external {}
    function f7() external {}
    function f8() external {}
    function f9() external {}
    function f10() external {}
    function f11() external {}
    function f12() external {}
    function f13() external {}
    function f14() external {}
    function f15() external {}
    function f16() external {}
    function f17() external {}
    function f18() external {}
    function f19() external {}
    function f20() external {}
    function f21() external {}
    function f22() external {}
    function f23() external {}
    function f24() external {}
    function f25() external {}
    function f26() external {}
    function f27() external {}
    function f28() external {}
    function f29() external {}
    function f30() external {}
    function f31() external {}
    function f32() external {}
    function f33() external {}
    function f34() external {}
    function f35() external {}
    function f36() external {}
    function f37() external {}
    function f38() external {}
    function f39() external {}

    fallback() external {
        assembly {
            sstore(0, 1)
        }
    }
}
