//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: empty 0 => 7
//@ run-call-fail: empty 1
//@ run-call: wrapped 0, 2 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: wrapped 3618502788666131106986593281521497120414687020801267626233049500247285301247, 3618502788666131106986593281521497120414687020801267626233049500247285301248 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: wrapped 3618502788666131106986593281521497120414687020801267626233049500247285301247, 3618502788666131106986593281521497120414687020801267626233049500247285301249 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract BoundsWrapping {
    // This pointer is never read before the loop. Versioning must not read its length when
    // count is zero, even though an eventual array access would load it.
    function empty(uint256 count) external pure returns (uint256) {
        uint256[] memory a;
        assembly {
            a := shl(64, 1)
        }
        for (uint256 i; i < count; ++i) {
            a[i] = 0;
        }
        return 7;
    }

    // The index 2^251 - 1 wraps a word store onto the array's own length. Its next access must
    // fail after the first store sets that length to zero, even though the old length admitted
    // both indices. The large-length guard must keep the original loop.
    function wrapped(uint256 start, uint256 end) external pure returns (uint256) {
        uint256[] memory a = new uint256[](1);
        assembly {
            mstore(a, not(0))
        }
        return _write(a, start, end);
    }

    // CHECK-LABEL: fn @_write(
    // CHECK: [[LENGTH:v[0-9]+]] = mload arg0
    // CHECK: lt {{v[0-9]+}}, 0x10000000000000000
    // CHECK: jumpi
    // CHECK: [[SHORT:v[0-9]+]] = lt [[LENGTH]], arg2
    // CHECK-NEXT: jumpi [[SHORT]]
    /// @custom:solar-inline never
    function _write(uint256[] memory a, uint256 start, uint256 end)
        private pure returns (uint256 length)
    {
        length = a.length;
        for (uint256 i = start; i < end; ++i) {
            a[i] = 0;
        }
    }
}
