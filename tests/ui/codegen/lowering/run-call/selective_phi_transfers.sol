//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 0, false, 4096 => 28
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 0, false, 4096 => 37
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, 0, false, 4096 => 50
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 0, false, 4096 => 59
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 1, false, 4096 => 28
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 1, false, 4096 => 29
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, 1, false, 4096 => 42
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 1, false, 4096 => 61
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 2, false, 4096 => 28
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 2, false, 4096 => 36
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, 2, false, 4096 => 52
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 2, false, 4096 => 58
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 3, false, 4096 => 28
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 3, false, 4096 => 38
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, 3, false, 4096 => 56
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 3, false, 4096 => 73
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 0, true, 4097 => 6327
//@ run-call: run [115792089237316195423570985008687907853269984665640564039457584007913129639935, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 0, true, 4097 => 6
//@ run-call: run [115792089237316195423570985008687907853269984665640564039457584007913129639935, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 3, 3, false, 4096 => 43

// One pressure branch must not force the independent small loop into memory.
// The loop needs simultaneous swaps, a three-cycle, duplicate sources and a
// newly computed incoming value. Each iteration must read all incoming values
// before replacing the corresponding loop values.
contract SelectivePhiTransfers {
    function run(uint256[18] calldata values, uint256 rounds, uint256 pattern, bool heavy, uint256 dst)
        external pure returns (uint256 result)
    {
        assembly {
            switch heavy
            case 0 {
                let x := calldataload(values)
                let y := calldataload(add(values, 32))
                let z := calldataload(add(values, 64))
                for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                    switch pattern
                    case 0 { let oldX := x x := y y := oldX }
                    case 1 { let oldX := x x := y y := z z := oldX }
                    case 2 { let oldY := y x := y y := z z := oldY }
                    default { let oldX := x let next := add(x, y) x := next y := oldX }
                }
                result := add(add(add(x, mul(y, 3)), mul(z, 7)), mul(rounds, 11))
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
