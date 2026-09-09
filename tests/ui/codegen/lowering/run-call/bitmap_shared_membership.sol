//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 191, 4096 => 0, 1584
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 192, 4096 => 0, 1584
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 223, 4096 => 0, 1584
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 287, 4096 => 0, 1584
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 4096, 4096 => 99, 77616
//@ run-call: run [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 191, 4096 => 0, 0

// Computed values cross a recursive call. Early deaths leave a sparse bank;
// a distinct post-store load keeps each even value live across the writer.
// Destination191 gives the wrapped relative index MAX for the actual bank
// beginning at192. The +1 factor makes zero readback still check every root.
// Both bitmap selections skip holes. The selected words are loaded before
// the source store and restored before the distinct readback.
// CHECK-LABEL: @module BitmapSharedMembership_runtime
// CHECK: push 0x55555557
// CHECK-NEXT: dup 2
// CHECK-NEXT: shr
// CHECK-NEXT: push 1
// CHECK-NEXT: and
// CHECK-NEXT: mul
// CHECK-NEXT: push 6
// CHECK-NEXT: add
// CHECK-NEXT: push 5
// CHECK-NEXT: shl
// CHECK-NEXT: dup 1
// CHECK-NEXT: mload
// CHECK-NEXT: swap 2
// CHECK-NEXT: push 1
// CHECK-NEXT: add
// CHECK-NEXT: push 0x55555557
// CHECK-NEXT: dup 2
// CHECK-NEXT: shr
// CHECK-NEXT: push 1
// CHECK-NEXT: and
// CHECK-NEXT: mul
// CHECK-NEXT: push 6
// CHECK-NEXT: add
// CHECK-NEXT: push 5
// CHECK-NEXT: shl
// CHECK-NEXT: dup 1
// CHECK-NEXT: mload
// CHECK-NEXT: swap 5
// CHECK-NEXT: swap 1
// CHECK-NEXT: swap 4
// CHECK-NEXT: mstore
// CHECK-NEXT: mstore
// CHECK-NEXT: mstore
// CHECK-NEXT: push 0x424
// CHECK-NEXT: calldataload
// CHECK-NEXT: mload
contract BitmapSharedMembership {
    function run(uint256[32] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            function bounce(x, depth) -> result {
                result := add(x, depth)
                if depth { result := bounce(result, sub(depth, 1)) }
            }
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let a18 := mul(calldataload(add(values, 576)), 3)
            let a19 := mul(calldataload(add(values, 608)), 3)
            let a20 := mul(calldataload(add(values, 640)), 3)
            let a21 := mul(calldataload(add(values, 672)), 3)
            let a22 := mul(calldataload(add(values, 704)), 3)
            let a23 := mul(calldataload(add(values, 736)), 3)
            let a24 := mul(calldataload(add(values, 768)), 3)
            let a25 := mul(calldataload(add(values, 800)), 3)
            let a26 := mul(calldataload(add(values, 832)), 3)
            let a27 := mul(calldataload(add(values, 864)), 3)
            let a28 := mul(calldataload(add(values, 896)), 3)
            let a29 := mul(calldataload(add(values, 928)), 3)
            let a30 := mul(calldataload(add(values, 960)), 3)
            let a31 := mul(calldataload(add(values, 992)), 3)
            let payload := bounce(xor(a00, a31), and(destination, 1))
            let odd := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a01, a03), a05), a07), a09), a11), a13), a15), a17), a19), a21), a23), a25), a27), a29), a31)
            mstore(destination, payload)
            observed := mload(source)
            checksum := add(odd, add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(mul(a00, add(observed, 1)), mul(a02, add(observed, 1))), mul(a04, add(observed, 1))), mul(a06, add(observed, 1))), mul(a08, add(observed, 1))), mul(a10, add(observed, 1))), mul(a12, add(observed, 1))), mul(a14, add(observed, 1))), mul(a16, add(observed, 1))), mul(a18, add(observed, 1))), mul(a20, add(observed, 1))), mul(a22, add(observed, 1))), mul(a24, add(observed, 1))), mul(a26, add(observed, 1))), mul(a28, add(observed, 1))), mul(a30, add(observed, 1))))
        }
    }
}
