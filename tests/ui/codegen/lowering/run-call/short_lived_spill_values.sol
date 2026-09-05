//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: mix [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20] => 2825
//@ run-call: mix [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1] => 20

contract ShortLivedSpillValues {
    function mix(uint256[20] calldata values) external pure returns (uint256 result) {
        assembly {
            // Independent loads exceed the physical DUP window before the expression chains.
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
            // Each left operand stays live across an independent right-operand computation.
            let left0 := add(a00, a01)
            let right0 := add(a02, a03)
            let product0 := mul(left0, right0)
            let left1 := add(a04, a05)
            let right1 := add(a06, a07)
            let product1 := mul(left1, right1)
            let left2 := add(a08, a09)
            let right2 := add(a10, a11)
            let product2 := mul(left2, right2)
            let left3 := add(a12, a13)
            let right3 := add(a14, a15)
            let product3 := mul(left3, right3)
            let left4 := add(a16, a17)
            let right4 := add(a18, a19)
            let product4 := mul(left4, right4)
            result := add(add(product0, product1), add(product2, add(product3, product4)))
        }
    }
}
