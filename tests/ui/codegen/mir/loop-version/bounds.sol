//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: siftUp 7, 7 => [3, 7, 2, 10, 6, 105, 9], [2, 1, 5, 3, 4, 0, 6]
//@ run-call: siftUp 7, 5 => [3, 7, 102, 10, 6, 2, 9], [2, 1, 0, 3, 4, 5, 6]
//@ run-call: siftUp 1, 1 => [100], [0]
//@ run-call: siftUp 12, 12 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111], [2, 1, 5, 3, 4, 11, 6, 7, 8, 9, 10, 0]
//@ run-call: siftUp 12, 9 => [3, 7, 2, 10, 6, 105, 9, 5, 1, 8, 4, 0], [2, 1, 5, 3, 4, 0, 6, 7, 8, 9, 10, 11]
//@ run-call: siftUp 7, 9 => [3, 7, 2, 10, 6, 105, 9], [2, 1, 5, 3, 4, 0, 6]
//@ run-call: siftUp 16, 17 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111, 7, 3, 10, 6], [2, 1, 5, 3, 4, 11, 6, 7, 8, 9, 10, 0, 12, 13, 14, 15]
//@ run-call-fail: siftUp 3, 6 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: siftValues 7, 7 => [3, 7, 2, 10, 6, 105, 9]
//@ run-call: siftValues 7, 5 => [3, 7, 102, 10, 6, 2, 9]
//@ run-call: siftValues 1, 1 => [100]
//@ run-call: siftValues 12, 12 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111]
//@ run-call: siftValues 12, 9 => [3, 7, 2, 10, 6, 105, 9, 5, 1, 8, 4, 0]
//@ run-call: siftValues 7, 9 => [3, 7, 2, 10, 6, 105, 9]
//@ run-call: siftValues 16, 17 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111, 7, 3, 10, 6]
//@ run-call-fail: siftValues 3, 6 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: literalInLoop 1, [7, 0, 0, 0] => 3
pragma solidity ^0.8.0;

// A counted loop whose bound is not the length of the arrays it checks its accesses against
// gets a copy without those checks, behind a test that the bound does not exceed the lengths.
// A bound past a length runs the original loop, with its checks.
contract Bounds {
    // A heap's sift loop runs up to the queue's size `e`, while every access checks its index
    // against the length of the array that holds the queue. The copy runs behind
    // `e <= values.length`, after the pointer and loaded length pass the non-wrapping guard.
    // The fast copy reuses that length; its stores cannot reach the header and change it.
    // CHECK-LABEL: fn @siftValues(
    // CHECK: [[ARRAY:v[0-9]+]] = heap_floor
    // CHECK: [[BITS:v[0-9]+]] = or [[ARRAY]], [[LENGTH:v[0-9]+]]
    // CHECK-NEXT: [[SMALL:v[0-9]+]] = lt [[BITS]], 0x10000000000000000
    // CHECK-NEXT: jumpi [[SMALL]], [[BOUNDS:bb[0-9]+]], [[SLOW:bb[0-9]+]]
    // CHECK: [[BOUNDS]]:
    // CHECK-NEXT: [[SHORT:v[0-9]+]] = lt [[LENGTH]], arg1
    // CHECK-NEXT: jumpi [[SHORT]], [[SLOW]], [[FAST:bb[0-9]+]]
    // CHECK-NEXT: [[FAST]]:
    // CHECK-NOT: mload [[ARRAY]]
    // CHECK-NOT: lt {{v[0-9]+}}, [[LENGTH]]
    // CHECK: [[SLOW]]:
    // CHECK-NEXT: jump [[HEADER:bb[0-9]+]]
    // CHECK: [[HEADER]]:
    // CHECK: jumpi {{v[0-9]+}}, [[BODY:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: [[BODY]]:
    // CHECK: [[RELOADED:v[0-9]+]] = mload [[ARRAY]]
    // CHECK: lt {{v[0-9]+}}, [[RELOADED]]
    function siftValues(uint256 m, uint256 e) external pure returns (uint256[] memory values) {
        values = new uint256[](m);
        for (uint256 i; i < m; ++i) {
            values[i] = (i * 7) % 11;
        }
        values[0] = 100;
        uint256 p;
        for (uint256 c = 1; c < e; c = (p << 1) | 1) {
            uint256 h = c >> 1;
            uint256 cv = values[c];
            if (c + 1 < e) {
                uint256 rv = values[c + 1];
                if (cv > rv) {
                    cv = rv;
                    ++c;
                }
            }
            values[h] = cv;
            p = c;
        }
        values[p] = 100 + p;
    }

    // With a second array of runtime size, a store into one array may reach the other's length
    // word once inline assembly has enlarged a length past its allocation. Versioning must also
    // prove that the arrays' element stores cannot reach either length word.
    function siftUp(uint256 m, uint256 e)
        external
        pure
        returns (uint256[] memory values, uint256[] memory keys)
    {
        values = new uint256[](m);
        keys = new uint256[](m);
        for (uint256 i; i < m; ++i) {
            values[i] = (i * 7) % 11;
            keys[i] = i;
        }
        values[0] = 100;
        uint256 k0 = keys[0];
        uint256 p;
        for (uint256 c = 1; c < e; c = (p << 1) | 1) {
            uint256 h = c >> 1;
            uint256 cv = values[c];
            if (c + 1 < e) {
                uint256 rv = values[c + 1];
                if (cv > rv) {
                    cv = rv;
                    ++c;
                }
            }
            values[h] = cv;
            keys[h] = keys[c];
            p = c;
        }
        values[p] = 100 + p;
        keys[p] = k0;
    }

    /// A literal's allocation in the copied loop stays where inline assembly reads it.
    function literalInLoop(uint256 n, uint256[4] memory a) external pure returns (uint256 result) {
        for (uint256 i; i < n; ++i) {
            uint256 x = a[i];
            uint256 before;
            assembly {
                before := mload(0x40)
            }
            bytes memory s = "abc";
            bytes memory r = abi.encode(x, s);
            assembly {
                result := mload(before)
            }
            require(r.length == 128);
        }
    }
}
