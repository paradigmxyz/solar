//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: probe 7, 0 => 28
//@ run-call: probe 7, 256 => 540
//@ run-call: probe 11, 320 => 684
//@ run-call: probe 11, 64 => 172
//@ run-call: probe 0, 0 => 0

// Both nested static activations retain twenty calldata words across control flow.
// The callee reads a shifted window so shared caller/callee homes corrupt the result.
// Arguments are reused, duplicated at a call, and unused; the unknown writer can
// overlap private spill homes without changing the logical values.
contract StaticSpillArguments {
    function probe(uint256 seed, uint256 destination) external pure returns (uint256 result) {
        assembly {
            function inner(base, left, right, unused, target) -> answer {
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
                if eq(left, right) { answer := add(left, right) }
                mstore(and(target, 0x3e0), 0xdead)
                answer := add(answer, add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(a16, a17), add(a18, a19))))
            }
            function outer(base, salt, target, unused) -> answer {
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
                answer := inner(add(base, 32), salt, salt, 99, target)
                if salt { answer := add(answer, salt) }
                answer := add(answer, add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(a16, a17), add(a18, a19))))
            }
            result := outer(4, seed, destination, 77)
        }
    }
}
