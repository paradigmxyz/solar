//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: fixedScratch [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20] => 248
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 0 => 248
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 128 => 248
//@ run-call: run [20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1], 384 => 248
//@ run-call: run [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], 160 => 20

contract WriterSpillValues {
    function fixedScratch(uint256[20] calldata values) external pure returns (uint256) {
        return run(values, 0);
    }
    function run(uint256[20] calldata values, uint256 destination) public pure returns (uint256 result) {
        assembly {
            function write(base, target) -> selected {
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
                let heap := mload(0x40)
                mstore(0x40, add(heap, 96))
                // These computed operands die in stores disjoint from compiler-owned words.
                mstore(add(heap, 32), xor(a00, a19))
                mstore8(add(heap, 64), xor(a01, a18))
                // Unknown addresses can overlap the live homes and must keep the ordinary barrier.
                mstore(add(target, 32), add(a02, a17))
                selected := add(add(mload(add(heap, 32)), byte(0, mload(add(heap, 64)))), add(add(add(add(add(a00, a01), add(a02, a03)), add(add(a04, a05), add(a06, a07))), add(add(add(a08, a09), add(a10, a11)), add(add(a12, a13), add(a14, a15)))), add(add(a16, a17), add(a18, a19))))
            }
            result := write(values, destination)
        }
    }
}
