//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 0, 4096 => 6356, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 0, 4096 => 6346, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 0, 4096 => 6356, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 1, 4096 => 6357, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 1, 4096 => 6347, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 1, 4096 => 6357, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 0, 2, 4096 => 6358, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 1, 2, 4096 => 6348, true
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 3, 2, 4096 => 6358, true

// Returning recursion and an unknown-destination writer exercise the existing
// protected call path. Gas reads remain ordered; no numeric gas equality is
// required across compilers. Recursive calls must preserve the live values.
contract SelectivePhiCallControl {
    function run(uint256[18] calldata values, uint256 rounds, uint256 depth, uint256 dst)
        external view returns (uint256 result, bool gasOrdered)
    {
        assembly {
            function touch(n, where) -> tag {
                switch n
                case 0 { mstore(where, 0x77) tag := 1 }
                default { tag := add(touch(sub(n, 1), where), 1) }
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
            let beforeGas := gas()
            let tag := touch(depth, dst)
            gasOrdered := gt(beforeGas, gas())
            let digest := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, mul(a01, 2)), mul(a02, 3)), mul(a03, 4)), mul(a04, 5)), mul(a05, 6)), mul(a06, 7)), mul(a07, 8)), mul(a08, 9)), mul(a09, 10)), mul(a10, 11)), mul(a11, 12)), mul(a12, 13)), mul(a13, 14)), mul(a14, 15)), mul(a15, 16)), mul(a16, 17)), mul(a17, 18))
            let x := calldataload(values)
            let y := calldataload(add(values, 32))
            let z := calldataload(add(values, 64))
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                let oldX := x
                x := y
                y := z
                z := oldX
            }
            result := add(add(digest, tag), add(add(x, mul(y, 3)), mul(z, 7)))
        }
    }
}
