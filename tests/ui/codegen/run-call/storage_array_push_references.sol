//@ run-call: references 42 => 42, 84, 4096
//@ run-call: nestedPush 2, 64 => 64
//@ run-call: bytesPush => 71, 0x00
//@ run-call: bytesTransition => 0
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
}
