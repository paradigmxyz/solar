//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: rotate [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 0 => 2870
//@ run-call: rotate [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 1 => 2828
//@ run-call: rotate [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 2 => 2829
//@ run-call: rotate [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 3 => 2834

contract ParallelSpillCopies {
    function rotate(uint256[20] calldata values, uint256 rounds) external pure returns (uint256 result) {
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
            // Simultaneous loop phis include two disjoint cycles, fan-out and a literal.
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                let first := a00
                a00 := a01
                a01 := first
                let second := a02
                a02 := a03
                a03 := a04
                a04 := second
                let third := a05
                a05 := a06
                a06 := a07
                a07 := third
                a08 := a09
                a10 := 7
            }
            result := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, mul(a01, 2)), mul(a02, 3)), mul(a03, 4)), mul(a04, 5)), mul(a05, 6)), mul(a06, 7)), mul(a07, 8)), mul(a08, 9)), mul(a09, 10)), mul(a10, 11)), mul(a11, 12)), mul(a12, 13)), mul(a13, 14)), mul(a14, 15)), mul(a15, 16)), mul(a16, 17)), mul(a17, 18)), mul(a18, 19)), mul(a19, 20))
        }
    }
}
