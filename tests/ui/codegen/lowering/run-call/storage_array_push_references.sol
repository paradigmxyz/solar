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
//@[none] run-call: references 42 => 42, 84, 4096
//@[gas] run-call: references 42 => 42, 84, 4096
//@[size] run-call: references 42 => 42, 84, 4096
//@[none] run-call: nestedPush 2, 64 => 64
//@[gas] run-call: nestedPush 2, 64 => 64
//@[size] run-call: nestedPush 2, 64 => 64
//@[none] run-call: bytesPush => 71, 0x00
//@[gas] run-call: bytesPush => 71, 0x00
//@[size] run-call: bytesPush => 71, 0x00
//@[none] run-call: bytesTransition => 0
//@[gas] run-call: bytesTransition => 0
//@[size] run-call: bytesTransition => 0
//@[none] run-call: pushPreservesStorage => 42
//@[gas] run-call: pushPreservesStorage => 42
//@[size] run-call: pushPreservesStorage => 42
// ported-from: test/libsolidity/semanticTests/array/push/push_no_args_struct.sol
// ported-from: test/libsolidity/semanticTests/array/push/push_no_args_2d.sol
// ported-from: test/libsolidity/semanticTests/array/push/push_no_args_bytes.sol

contract StorageArrayPushReferences {
    struct Entry {
        uint256 value;
    }

    Entry[] private entries;
    uint256[][] private nestedValues;
    bytes private byteValues;

    function references(uint256 value) external returns (uint256, uint256, uint256) {
        Entry storage first = entries.push();
        first.value = value;
        entries.push().value = value * 2;
        entries.push().value = 4096;
        return (entries[0].value, entries[1].value, entries[2].value);
    }

    function nestedPush(uint256 index, uint256 value) external returns (uint256) {
        uint256[] storage values = nestedValues.push();
        for (uint256 i; i <= index; ++i) values.push();
        values[index] = value;
        return nestedValues[0][index];
    }

    function bytesPush() external returns (uint256, bytes1) {
        for (uint256 i; i < 70; ++i) byteValues.push(bytes1(uint8(i)));
        byteValues.push();
        return (byteValues.length, byteValues[70]);
    }

    function bytesTransition() external returns (uint256) {
        for (uint8 i = 1; i < 40; ++i) {
            byteValues.push(bytes1(i));
            if (byteValues.length != i || byteValues[byteValues.length - 1] != bytes1(i)) {
                return 0x1000 + i;
            }
        }
        for (uint8 i = 1; i < 40; ++i) {
            if (byteValues[i - 1] != bytes1(i)) return 0x1000000 + i;
        }
        return 0;
    }

    function pushPreservesStorage() external returns (uint256) {
        uint256 entrySlot;
        assembly {
            mstore(0, entries.slot)
            entrySlot := keccak256(0, 32)
            sstore(entrySlot, 42)
        }
        entries.push();
        return entries[0].value;
    }
}
