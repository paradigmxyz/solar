//@ codegen-matrix: standard
//@ run-call: sparse [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], 191, 255 => 1176
//@ run-call: sparse [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], 192, 256 => 1176
//@ run-call: sparse [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], 223, 255 => 1176
//@ run-call: sparse [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], 4096, 0 => 1176
//@ run-call-fail: sparse [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], 115792089237316195423570985008687907853269984665640564039457584007913129639935, 255

// Early deaths leave holes among the initialized homes of values retained across the store.
contract BitmapByteProtection {
    function sparse(uint256[48] calldata values, uint256 offset, uint256 byteValue) external pure returns (uint256) {
        assembly {
            let a00 := calldataload(add(values, 0))
            let a01 := calldataload(add(values, 32))
            let a02 := calldataload(add(values, 64))
            let a03 := calldataload(add(values, 96))
            let a04 := calldataload(add(values, 128))
            let a05 := calldataload(add(values, 160))
            let a06 := calldataload(add(values, 192))
            let a07 := calldataload(add(values, 224))
            let a08 := calldataload(add(values, 256))
            let a09 := calldataload(add(values, 288))
            let a10 := calldataload(add(values, 320))
            let a11 := calldataload(add(values, 352))
            let a12 := calldataload(add(values, 384))
            let a13 := calldataload(add(values, 416))
            let a14 := calldataload(add(values, 448))
            let a15 := calldataload(add(values, 480))
            let a16 := calldataload(add(values, 512))
            let a17 := calldataload(add(values, 544))
            let a18 := calldataload(add(values, 576))
            let a19 := calldataload(add(values, 608))
            let a20 := calldataload(add(values, 640))
            let a21 := calldataload(add(values, 672))
            let a22 := calldataload(add(values, 704))
            let a23 := calldataload(add(values, 736))
            let a24 := calldataload(add(values, 768))
            let a25 := calldataload(add(values, 800))
            let a26 := calldataload(add(values, 832))
            let a27 := calldataload(add(values, 864))
            let a28 := calldataload(add(values, 896))
            let a29 := calldataload(add(values, 928))
            let a30 := calldataload(add(values, 960))
            let a31 := calldataload(add(values, 992))
            let a32 := calldataload(add(values, 1024))
            let a33 := calldataload(add(values, 1056))
            let a34 := calldataload(add(values, 1088))
            let a35 := calldataload(add(values, 1120))
            let a36 := calldataload(add(values, 1152))
            let a37 := calldataload(add(values, 1184))
            let a38 := calldataload(add(values, 1216))
            let a39 := calldataload(add(values, 1248))
            let a40 := calldataload(add(values, 1280))
            let a41 := calldataload(add(values, 1312))
            let a42 := calldataload(add(values, 1344))
            let a43 := calldataload(add(values, 1376))
            let a44 := calldataload(add(values, 1408))
            let a45 := calldataload(add(values, 1440))
            let a46 := calldataload(add(values, 1472))
            let a47 := calldataload(add(values, 1504))
            let odd := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a01, a03), a05), a07), a09), a11), a13), a15), a17), a19), a21), a23), a25), a27), a29), a31), a33), a35), a37), a39), a41), a43), a45), a47)
            mstore8(offset, byteValue)
            mstore(0, add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(odd, a00), a02), a04), a06), a08), a10), a12), a14), a16), a18), a20), a22), a24), a26), a28), a30), a32), a34), a36), a38), a40), a42), a44), a46))
            return(0, 32)
        }
    }
}
