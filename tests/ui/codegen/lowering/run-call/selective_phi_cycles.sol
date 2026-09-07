//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, false, false, 4096 => 285
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, false, false, 4096 => 249
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, false, false, 4096 => 222
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 8, false, false, 4096 => 249
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 9, false, false, 4096 => 285
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, true, false, 4096 => 258
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, true, false, 4096 => 239
//@ run-call: run [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, false, false, 4096 => 9
//@ run-call: run [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, false, false, 4096 => 8
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, false, true, 4097 => 6327

// Nine evolving values plus the loop counter exceed the selective eight-word
// budget. A separate pressure branch prevents the ordinary all-stack fast path.
// Any accepted loop promotion must therefore leave a mixed stack/home edge.
contract SelectivePhiCycles {
    function run(uint256[18] calldata values, uint256 rounds, bool fanout, bool heavy, uint256 dst)
        external pure returns (uint256 result)
    {
        assembly {
            switch heavy
            case 0 {
                let x0 := calldataload(add(values, 0))
                let x1 := calldataload(add(values, 32))
                let x2 := calldataload(add(values, 64))
                let x3 := calldataload(add(values, 96))
                let x4 := calldataload(add(values, 128))
                let x5 := calldataload(add(values, 160))
                let x6 := calldataload(add(values, 192))
                let x7 := calldataload(add(values, 224))
                let x8 := calldataload(add(values, 256))
                for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                    let last := x0
                    if fanout { last := x1 }
                    x0 := x1
                    x1 := x2
                    x2 := x3
                    x3 := x4
                    x4 := x5
                    x5 := x6
                    x6 := x7
                    x7 := x8
                    x8 := last
                }
                result := add(add(add(add(add(add(add(add(x0, mul(x1, 2)), mul(x2, 3)), mul(x3, 4)), mul(x4, 5)), mul(x5, 6)), mul(x6, 7)), mul(x7, 8)), mul(x8, 9))
            }
            default {
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
                mstore(dst, 7)
                result := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, mul(a01, 2)), mul(a02, 3)), mul(a03, 4)), mul(a04, 5)), mul(a05, 6)), mul(a06, 7)), mul(a07, 8)), mul(a08, 9)), mul(a09, 10)), mul(a10, 11)), mul(a11, 12)), mul(a12, 13)), mul(a13, 14)), mul(a14, 15)), mul(a15, 16)), mul(a16, 17)), mul(a17, 18))
            }
        }
    }
}
