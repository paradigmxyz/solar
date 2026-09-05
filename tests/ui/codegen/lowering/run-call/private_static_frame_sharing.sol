//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64] => 6240

contract PrivateStaticFrameSharing {
    function run(uint256[64] calldata values) external pure returns (uint256 result) {
        assembly {
            function leaf(base) -> value {
                let a00 := calldataload(add(base, 0))
                let a01 := calldataload(add(base, 32))
                let a02 := calldataload(add(base, 64))
                let a03 := calldataload(add(base, 96))
                let a04 := calldataload(add(base, 128))
                let a05 := calldataload(add(base, 160))
                let a06 := calldataload(add(base, 192))
                let a07 := calldataload(add(base, 224))
                let a08 := calldataload(add(base, 256))
                let a09 := calldataload(add(base, 288))
                let a10 := calldataload(add(base, 320))
                let a11 := calldataload(add(base, 352))
                let a12 := calldataload(add(base, 384))
                let a13 := calldataload(add(base, 416))
                let a14 := calldataload(add(base, 448))
                let a15 := calldataload(add(base, 480))
                let a16 := calldataload(add(base, 512))
                let a17 := calldataload(add(base, 544))
                let a18 := calldataload(add(base, 576))
                let a19 := calldataload(add(base, 608))
                let a20 := calldataload(add(base, 640))
                let a21 := calldataload(add(base, 672))
                let a22 := calldataload(add(base, 704))
                let a23 := calldataload(add(base, 736))
                let a24 := calldataload(add(base, 768))
                let a25 := calldataload(add(base, 800))
                let a26 := calldataload(add(base, 832))
                let a27 := calldataload(add(base, 864))
                let a28 := calldataload(add(base, 896))
                let a29 := calldataload(add(base, 928))
                let a30 := calldataload(add(base, 960))
                let a31 := calldataload(add(base, 992))
                value := add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(add(add(a16, a17), add(a18, a19)), add(add(a20, a21), add(a22, a23))), add(add(add(a24, a25), add(a26, a27)), add(add(a28, a29), add(a30, a31)))))
            }
            // The recursive bridge has no static frame but must propagate its caller's end.
            function recurse(base, depth) -> value {
                switch depth
                case 0 { value := leaf(base) }
                default { value := add(leaf(base), recurse(base, sub(depth, 1))) }
            }
            function parent(base) -> value {
                let a00 := calldataload(add(base, 0))
                let a01 := calldataload(add(base, 32))
                let a02 := calldataload(add(base, 64))
                let a03 := calldataload(add(base, 96))
                let a04 := calldataload(add(base, 128))
                let a05 := calldataload(add(base, 160))
                let a06 := calldataload(add(base, 192))
                let a07 := calldataload(add(base, 224))
                let a08 := calldataload(add(base, 256))
                let a09 := calldataload(add(base, 288))
                let a10 := calldataload(add(base, 320))
                let a11 := calldataload(add(base, 352))
                let a12 := calldataload(add(base, 384))
                let a13 := calldataload(add(base, 416))
                let a14 := calldataload(add(base, 448))
                let a15 := calldataload(add(base, 480))
                let a16 := calldataload(add(base, 512))
                let a17 := calldataload(add(base, 544))
                let a18 := calldataload(add(base, 576))
                let a19 := calldataload(add(base, 608))
                let a20 := calldataload(add(base, 640))
                let a21 := calldataload(add(base, 672))
                let a22 := calldataload(add(base, 704))
                let a23 := calldataload(add(base, 736))
                let a24 := calldataload(add(base, 768))
                let a25 := calldataload(add(base, 800))
                let a26 := calldataload(add(base, 832))
                let a27 := calldataload(add(base, 864))
                let a28 := calldataload(add(base, 896))
                let a29 := calldataload(add(base, 928))
                let a30 := calldataload(add(base, 960))
                let a31 := calldataload(add(base, 992))
                value := add(recurse(add(base, 1024), 2), add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(add(add(a16, a17), add(a18, a19)), add(add(a20, a21), add(a22, a23))), add(add(add(a24, a25), add(a26, a27)), add(add(a28, a29), add(a30, a31))))))
            }
            function sibling(base) -> value {
                let a00 := calldataload(add(base, 0))
                let a01 := calldataload(add(base, 32))
                let a02 := calldataload(add(base, 64))
                let a03 := calldataload(add(base, 96))
                let a04 := calldataload(add(base, 128))
                let a05 := calldataload(add(base, 160))
                let a06 := calldataload(add(base, 192))
                let a07 := calldataload(add(base, 224))
                let a08 := calldataload(add(base, 256))
                let a09 := calldataload(add(base, 288))
                let a10 := calldataload(add(base, 320))
                let a11 := calldataload(add(base, 352))
                let a12 := calldataload(add(base, 384))
                let a13 := calldataload(add(base, 416))
                let a14 := calldataload(add(base, 448))
                let a15 := calldataload(add(base, 480))
                let a16 := calldataload(add(base, 512))
                let a17 := calldataload(add(base, 544))
                let a18 := calldataload(add(base, 576))
                let a19 := calldataload(add(base, 608))
                let a20 := calldataload(add(base, 640))
                let a21 := calldataload(add(base, 672))
                let a22 := calldataload(add(base, 704))
                let a23 := calldataload(add(base, 736))
                let a24 := calldataload(add(base, 768))
                let a25 := calldataload(add(base, 800))
                let a26 := calldataload(add(base, 832))
                let a27 := calldataload(add(base, 864))
                let a28 := calldataload(add(base, 896))
                let a29 := calldataload(add(base, 928))
                let a30 := calldataload(add(base, 960))
                let a31 := calldataload(add(base, 992))
                value := mul(2, add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(add(add(a16, a17), add(a18, a19)), add(add(a20, a21), add(a22, a23))), add(add(add(a24, a25), add(a26, a27)), add(add(a28, a29), add(a30, a31))))))
            }
            // Parent and leaf coexist; sibling executes in a separate activation.
            result := add(parent(values), sibling(values))
        }
    }
}
