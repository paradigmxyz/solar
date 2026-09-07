//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, false, 4096 => 28
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, false, 4096 => 18
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, false, 4096 => 20
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, false, 4096 => 28
//@ run-call: run [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, false, 4096 => 7
//@ run-call: run [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, false, 4096 => 1
//@ run-call: run [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, false, 4096 => 3
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, true, 4096 => 6327

// The loop writer consumes the old first value, which remains live for the
// rotation. The unrelated branch supplies the ordinary spill pressure.
contract SelectivePhiWriter {
    function run(uint256[18] calldata values, uint256 rounds, bool heavy, uint256 dst)
        external pure returns (uint256 result)
    {
        assembly {
            switch heavy
            case 0 {
                let a := calldataload(values)
                let b := calldataload(add(values, 32))
                let c := calldataload(add(values, 64))
                for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                    mstore(dst, a)
                    let oldA := a
                    a := b
                    b := c
                    c := oldA
                }
                result := add(add(a, mul(b, 3)), mul(c, 7))
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
