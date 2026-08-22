//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: slots() => 0, 3, 4, 7, 8, 10
//@[none, gas, size] run-call: preserveNeighbors() => 0x000000000000000006, 11, 0x0000000000000000000000000000000003, 12, 3, 13
//@[none, gas, size] run-call: copyDoesNotOverlapSource() => 6, 1, 6, 0x0000000000000000000000000000000000000000000000000000000000000000

contract StorageFixedArrayLayout {
    bytes9[7] private bytes9Values;
    uint256 private afterBytes9;
    bytes17[3] private bytes17Values;
    uint256 private afterBytes17;
    uint128[3] private uint128Values;
    uint256 private afterUint128;
    bytes32[10] private destination;

    function slots()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        uint256 bytes9Slot;
        uint256 afterBytes9Slot;
        uint256 bytes17Slot;
        uint256 afterBytes17Slot;
        uint256 uint128Slot;
        uint256 afterUint128Slot;
        assembly {
            bytes9Slot := bytes9Values.slot
            afterBytes9Slot := afterBytes9.slot
            bytes17Slot := bytes17Values.slot
            afterBytes17Slot := afterBytes17.slot
            uint128Slot := uint128Values.slot
            afterUint128Slot := afterUint128.slot
        }
        return (
            bytes9Slot,
            afterBytes9Slot,
            bytes17Slot,
            afterBytes17Slot,
            uint128Slot,
            afterUint128Slot
        );
    }

    function preserveNeighbors()
        external
        returns (bytes9, uint256, bytes17, uint256, uint128, uint256)
    {
        bytes9Values[6] = bytes9(uint72(6));
        afterBytes9 = 11;
        bytes17Values[2] = bytes17(uint136(3));
        afterBytes17 = 12;
        uint128Values[2] = 3;
        afterUint128 = 13;
        return (
            bytes9Values[6],
            afterBytes9,
            bytes17Values[2],
            afterBytes17,
            uint128Values[2],
            afterUint128
        );
    }

    function copyDoesNotOverlapSource()
        external
        returns (uint72 sourceTail, uint72 first, uint72 last, bytes32 cleared)
    {
        for (uint256 i; i < bytes9Values.length; ++i) {
            bytes9Values[i] = bytes9(uint72(i));
        }
        destination[8] = destination[9] = bytes8(uint64(2));
        destination = bytes9Values;
        return (
            uint72(bytes9Values[6]),
            uint72(bytes9(destination[1])),
            uint72(bytes9(destination[6])),
            destination[9]
        );
    }
}
