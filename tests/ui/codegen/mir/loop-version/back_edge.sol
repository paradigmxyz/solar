//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: shift [1, 2, 3], [7, 8], [0, 0, 0], 1 => [9, 10, 11]
//@ run-call: shift [], [7, 8], [], 18446744073709551616 => []
//@ run-call-fail: shift [1], [7, 8], [0], 18446744073709551616 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: shiftFar [], [], 0 => []

// The loop reads `b[k]`, at an index that does not change, and `b`'s length
// only after its trip test and the bounds check of `k`. A versioning test
// before the loop would read them even when the loop never runs, or fails
// first, and expand memory to an offset the caller chooses. So the test runs on
// the back edge, after the first iteration read the same words, and a loop
// that does not run or fails reads nothing more than the source does.
// CHECK-LABEL: fn @shift(
// CHECK: icall @decode_calldata_type, 68
// CHECK-NOT: mload
// CHECK: jump [[HEADER:bb[0-9]+]]
// CHECK: [[HEADER]]:
// CHECK: mstore
// CHECK: shr {{v[0-9]+}}, 64
// CHECK: jumpi {{v[0-9]+}}, {{bb[0-9]+}}, [[HEADER]]

contract Test {
    function shift(uint256[] memory a, uint256[] memory b, uint256[] memory c, uint256 k)
        external
        pure
        returns (uint256[] memory)
    {
        for (uint256 i; i < a.length; ++i) {
            c[i] = a[i] + b[k];
        }
        return c;
    }

    // `b` points far past memory, so reading its length runs out of gas: only a
    // loop that runs may read it.
    function shiftFar(uint256[] memory a, uint256[] memory c, uint256 k)
        external
        pure
        returns (uint256[] memory)
    {
        uint256[] memory b;
        assembly {
            b := shl(64, 1)
        }
        for (uint256 i; i < a.length; ++i) {
            c[i] = a[i] + b[k];
        }
        return c;
    }
}
