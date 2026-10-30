//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: fill 0x0000, 0, 0x01 => true
//@ run-call: fill 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0xff => true
//@ run-call-fail: fill 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0x01 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: scatter 0x0000, 0, 0x0102000000000000000000000000000000000000000000000000000000000000, 2, 0 => true
//@ run-call: scatter 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0x0102000000000000000000000000000000000000000000000000000000000000, 0, 0 => true
//@ run-call: scatter 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0x0102000000000000000000000000000000000000000000000000000000000000, 1, 0 => true
//@ run-call-fail: scatter 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0x0102000000000000000000000000000000000000000000000000000000000000, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: scatter 0x0000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0xffff000000000000000000000000000000000000000000000000000000000000, 2, 0 => true
//@ run-call: scatter 0x, 1, 0x0102000000000000000000000000000000000000000000000000000000000000, 0, 0x10000000000000000 => true

// A forged length admits an offset whose address wraps onto the length word. Writing its
// first byte can shorten the buffer, so the next iteration must recheck the changed length.
// A fill of 0xff leaves the length unchanged and must still succeed on the fallback path.
contract Test {
    // Both the word store and the original byte loop must survive the wrap guard.
    // CHECK-LABEL: fn @fill(
    // CHECK: [[OBJECT:v[0-9]+]] = icall @decode_calldata_type
    // CHECK: [[BASE:v[0-9]+]] = add [[OBJECT]], arg1
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK-NEXT: [[DATA:v[0-9]+]] = add [[OBJECT]], 32
    // CHECK-NEXT: [[WRAPS:v[0-9]+]] = lt [[ADDRESS]], [[DATA]]
    // CHECK-NEXT: jumpi [[WRAPS]], [[LOOP:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: mstore [[ADDRESS]],
    // CHECK: [[LOOP]]:
    // CHECK: mload [[OBJECT]]
    // CHECK: mstore8
    function fill(bytes memory buffer, uint256 offset, bytes1 value) external pure returns (bool) {
        assembly {
            mstore(buffer, not(0))
        }
        for (uint256 k; k < 2; ++k) {
            buffer[offset + k] = value;
        }
        return true;
    }

    // Even a forged pointer that would exhaust gas on any load is unused for zero iterations.
    function scatter(bytes memory buffer, uint256 offset, bytes32 value, uint256 count, uint256 pointer)
        external pure returns (bool)
    {
        assembly {
            mstore(buffer, not(0))
            if pointer { buffer := pointer }
        }
        _scatter(buffer, offset, value, count);
        return true;
    }

    // Zero iterations bypass both the guarded fast store and the retained byte loop.
    // CHECK-LABEL: fn @_scatter(
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg3, 0
    // CHECK-NEXT: jumpi [[EMPTY]], [[EXIT:bb[0-9]+]], [[SETUP:bb[0-9]+]]
    // CHECK: [[SETUP]]:
    // CHECK-NOT: mload
    // CHECK: [[BASE:v[0-9]+]] = add arg0, arg1
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK-NEXT: [[DATA:v[0-9]+]] = add arg0, 32
    // CHECK-NEXT: [[WRAPS:v[0-9]+]] = lt [[ADDRESS]], [[DATA]]
    // CHECK-NEXT: jumpi [[WRAPS]], [[LOOP:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: mstore [[ADDRESS]],
    // CHECK: jump [[EXIT]]
    // CHECK: [[LOOP]]:
    // CHECK: phi [{{bb[0-9]+}}: 0]
    // CHECK: mload arg0
    // CHECK: mstore8
    // CHECK: [[EXIT]]:
    // CHECK-NEXT: ret
    /// @custom:solar-inline never
    function _scatter(bytes memory buffer, uint256 offset, bytes32 value, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            buffer[offset + k] = value[k];
        }
    }
}
