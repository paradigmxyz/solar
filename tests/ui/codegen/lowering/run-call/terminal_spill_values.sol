//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20] => 231
//@ run-call: run [20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1] => 232
//@ run-call: run [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1] => 20

contract TerminalSpillValues {
    function run(uint256[20] calldata values) external pure returns (uint256 result) {
        assembly {
            function choose(base) -> selected {
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
                // This condition dies at its branch while all loaded words remain live.
                if lt(a00, a19) { selected := add(a00, a19) }
                // This condition stays live after its branch and must retain its home.
                let retained := gt(a01, a18)
                if retained { selected := add(selected, add(a01, a18)) }
                selected := add(add(selected, retained), add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(a16, a17), add(a18, a19))))
            }
            result := choose(values)
        }
    }
}
