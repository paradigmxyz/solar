//@ codegen-matrix: standard dump
//@ compile-flags: -Zvalidate-ir=true
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: f 0x0000000000000000000000000000000000000000000000000000000000000011, 27 => 0x80, 0x0000000000000000000000000000000000000000
//@ run-call: f 0x0000000000000000000000000000000000000000000000000000000000000011, 31 => 0x80, 0x0000000000000000000000000000000000000000
//@ run-call: f 0x0000000000000000000000000000000000000000000000000000000000000011, 32 => 0x80, 0x0000000000000000000000000000000000000000
//@ run-call-fail: f 0x0000000000000000000000000000000000000000000000000000000000000011, 33

// An assembly `mload(0x40)` reads the free memory pointer as an `i256` word. The `fmp` read after
// the join reads a `ptr<memory>` of equal value, so load-pre must keep it instead of replacing it
// with the word.
contract LoadPreKeepsPointerType {
    // CHECK-LABEL: fn @f
    // CHECK: = mload 64
    // CHECK: = heap_floor
    function f(bytes32 h, uint8 v) external pure returns (uint256 p, address a) {
        assembly {
            p := mload(0x40)
        }
        if (v > 30) {
            require(v == 31 || v == 32);
        }
        a = ecrecover(h, v, 0, 0);
    }
}
