//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: apart 0x0102030405, 0xaaaaaaaaaa, 3 => 0x010203aaaa
//@ run-call-fail: apart 0x01, 0xaaaaaaaaaa, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: lengthInside 0x01000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000000, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: lengthInside 0x00000000000000000000000000000000000000000000000000000000000000030100000000000000000000000000000000000000000000000000000000000000, 8 => 0x01000000000000000000000000000000000000000000000000000000000000030100000000000000000000000000000000000000000000000000000000000000
//@ run-call: lengthInside 0x00000000000000000000000000000000000000000000000000000000000000030100000000000000000000000000000000000000000000000000000000000000, 0 => 0x00000000000000000000000000000000000000000000000000000000000000030100000000000000000000000000000000000000000000000000000000000000
//@ run-call: apartDown 0x0102030405, 0xaaaaaaaaaa, 3 => 0x010203aaaa
//@ run-call-fail: lengthInsideDown 0x00010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: lengthInsideDown 0x00000000000000000000000000000000000000000000000000000000000000081111111111111111000000000000000000000000000000000000000000000000, 8 => 0x11111111111111110000000000000000000000000000000000000000000000081111111111111111000000000000000000000000000000000000000000000000

// A copy between buffers that may overlap reloads its source's length after every store, as the
// loop's bounds checks read it, while the copy that replaces it reads the length once. The
// destination may cover that length word, which lies below the source's data, so the run-time
// test keeps the loop whenever the destination range meets the source from its length word to
// the end of the copied range.
contract Test {
    function apart(bytes memory src, bytes memory dst, uint256 count)
        external
        pure
        returns (bytes memory)
    {
        _copy(dst, src, count);
        return dst;
    }

    /// Copies into `dst` from a source whose length is `dst`'s first data word.
    function lengthInside(bytes memory dst, uint256 count) external pure returns (bytes memory) {
        bytes memory src;
        assembly ("memory-safe") {
            src := add(dst, 32)
        }
        _copy(dst, src, count);
        return dst;
    }

    function apartDown(bytes memory src, bytes memory dst, uint256 count)
        external
        pure
        returns (bytes memory)
    {
        _copyDown(dst, src, count);
        return dst;
    }

    function lengthInsideDown(bytes memory dst, uint256 count)
        external
        pure
        returns (bytes memory)
    {
        bytes memory src;
        assembly ("memory-safe") {
            src := add(dst, 32)
        }
        _copyDown(dst, src, count);
        return dst;
    }

    // The loop stays when `dst[0, count)` meets `src` from its length word to `src[count]`.
    // CHECK-LABEL: fn @_copy(
    // CHECK: [[FROM:v[0-9]+]] = add arg1, 32
    // CHECK-NEXT: [[TO:v[0-9]+]] = add arg0, 32
    // CHECK-NEXT: [[END:v[0-9]+]] = add [[FROM]], arg2
    // CHECK-NEXT: [[LAST:v[0-9]+]] = add [[TO]], arg2
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt [[TO]], [[END]]
    // CHECK-NEXT: [[ABOVE:v[0-9]+]] = lt arg1, [[LAST]]
    // CHECK-NEXT: [[MEETS:v[0-9]+]] = and [[BELOW]], [[ABOVE]]
    // CHECK-NEXT: jumpi [[MEETS]], {{bb[0-9]+}}, {{bb[0-9]+}}
    // CHECK: mcopy [[TO]], [[FROM]], arg2
    // CHECK: mstore8
    function _copy(bytes memory dst, bytes memory src, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            dst[k] = src[k];
        }
    }

    // So does a downward copy's.
    // CHECK-LABEL: fn @_copyDown(
    // CHECK: [[FROM:v[0-9]+]] = add arg1, 32
    // CHECK-NEXT: [[TO:v[0-9]+]] = add arg0, 32
    // CHECK-NEXT: [[END:v[0-9]+]] = add [[FROM]], arg2
    // CHECK-NEXT: [[LAST:v[0-9]+]] = add [[TO]], arg2
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt [[TO]], [[END]]
    // CHECK-NEXT: [[ABOVE:v[0-9]+]] = lt arg1, [[LAST]]
    // CHECK-NEXT: [[MEETS:v[0-9]+]] = and [[BELOW]], [[ABOVE]]
    // CHECK-NEXT: jumpi [[MEETS]], {{bb[0-9]+}}, {{bb[0-9]+}}
    // CHECK: mcopy [[TO]], [[FROM]], arg2
    // CHECK: mstore8
    function _copyDown(bytes memory dst, bytes memory src, uint256 count) private pure {
        for (uint256 k = count; k > 0; --k) {
            dst[k - 1] = src[k - 1];
        }
    }
}
