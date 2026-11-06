//@ codegen-matrix: standard
//@ run-call: check => true
//@ run-call-fail: read 18446744073709551616 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract LargeStorageArrays {
    struct Wide {
        uint256[1 << 160] values;
        uint256 tail;
    }
    Wide[1 << 64] private wide;
    uint8[(1 << 64) + 1] private packed;
    uint256 private afterPacked;

    function check() external returns (bool) {
        uint256 index = (1 << 64) - 1;
        wide[index].values[(1 << 160) - 1] = 7;
        wide[index].tail = 11;
        packed[1 << 64] = 13;
        afterPacked = 17;
        uint256 packedSlot;
        uint256 afterSlot;
        uint256 stored;
        assembly {
            packedSlot := packed.slot
            afterSlot := afterPacked.slot
            stored := sload(add(mul(index, add(shl(160, 1), 1)), sub(shl(160, 1), 1)))
        }
        return packedSlot == (1 << 224) + (1 << 64)
            && afterSlot == packedSlot + (1 << 59) + 1
            && stored == 7 && wide[index].values[(1 << 160) - 1] == 7
            && wide[index].tail == 11 && packed[1 << 64] == 13 && afterPacked == 17;
    }

    function read(uint256 index) external view returns (uint256) {
        return wide[index].tail;
    }
}
