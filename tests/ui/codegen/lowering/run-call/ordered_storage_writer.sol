//@ codegen-matrix: standard
//@ run-call: OrderedStorageWriter::run 4096, 4096, 3735928559 => 3735928559, 5753854965885600108575829560559299546819203860
//@ run-call: OrderedStorageWriter::run 8192, 8192, 3735928559 => 3735928559, 5753854965885600108575829560559299546819203860
//@ run-call: OrderedStorageWriter::run 4096, 8192, 3735928559 => 0, 5753854965885600108575829560559299546819203860
//@ run-call: OrderedStorageWriter::run 8192, 4096, 3735928559 => 0, 5753854965885600108575829560559299546819203860
//@ run-call: OrderedStorageWriter::run 4096, 4096, 0 => 0, 5753854965885600108575829560559299546819203860
//@ run-call: OrderedStorageWriter::run 8192, 8192, 0 => 0, 5753854965885600108575829560559299546819203860

// Keep distinct storage words live across an unknown memory write.
// Packing in source order detects lost, duplicated, or permuted values.
// The readback addresses used below are disjoint from compiler memory.
contract OrderedStorageWriter {
    constructor() {
        assembly {
            sstore(0, 1)
            sstore(1, 2)
            sstore(2, 3)
            sstore(3, 4)
            sstore(4, 5)
            sstore(5, 6)
            sstore(6, 7)
            sstore(7, 8)
            sstore(8, 9)
            sstore(9, 10)
            sstore(10, 11)
            sstore(11, 12)
            sstore(12, 13)
            sstore(13, 14)
            sstore(14, 15)
            sstore(15, 16)
            sstore(16, 17)
            sstore(17, 18)
            sstore(18, 19)
            sstore(19, 20)
        }
    }

    function run(uint256 destination, uint256 source, uint256 value)
        external view returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := sload(0)
            let a01 := sload(1)
            let a02 := sload(2)
            let a03 := sload(3)
            let a04 := sload(4)
            let a05 := sload(5)
            let a06 := sload(6)
            let a07 := sload(7)
            let a08 := sload(8)
            let a09 := sload(9)
            let a10 := sload(10)
            let a11 := sload(11)
            let a12 := sload(12)
            let a13 := sload(13)
            let a14 := sload(14)
            let a15 := sload(15)
            let a16 := sload(16)
            let a17 := sload(17)
            let a18 := sload(18)
            let a19 := sload(19)
            mstore(destination, value)
            checksum := a00
            checksum := or(shl(8, checksum), a01)
            checksum := or(shl(8, checksum), a02)
            checksum := or(shl(8, checksum), a03)
            checksum := or(shl(8, checksum), a04)
            checksum := or(shl(8, checksum), a05)
            checksum := or(shl(8, checksum), a06)
            checksum := or(shl(8, checksum), a07)
            checksum := or(shl(8, checksum), a08)
            checksum := or(shl(8, checksum), a09)
            checksum := or(shl(8, checksum), a10)
            checksum := or(shl(8, checksum), a11)
            checksum := or(shl(8, checksum), a12)
            checksum := or(shl(8, checksum), a13)
            checksum := or(shl(8, checksum), a14)
            checksum := or(shl(8, checksum), a15)
            checksum := or(shl(8, checksum), a16)
            checksum := or(shl(8, checksum), a17)
            checksum := or(shl(8, checksum), a18)
            checksum := or(shl(8, checksum), a19)
            observed := mload(source)
        }
    }
}
