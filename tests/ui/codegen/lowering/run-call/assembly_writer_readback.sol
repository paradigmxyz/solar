//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 192, 192 => 0xdeadbeef, 171
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 0xdeadbeef, 171

// Reading the written word must preserve both the source memory value and live operands.
// The 4096 address exercises the same write and read away from the original spill homes.
contract WriterReadback18 {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
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
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}
