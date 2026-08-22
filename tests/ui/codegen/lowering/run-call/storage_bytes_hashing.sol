//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: mappingEmpty() => true
//@[none, gas, size] run-call: mappingLong(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: mappingShort(bytes31) 0x00000000000000000000000000000000000000000000000000000000000000 => true
//@[none, gas, size] run-call: mappingWord(bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000 => true
//@[none, gas, size] run-call: mappingTwoWords(bytes32,bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: mappingReference(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: nestedMapping(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: structField(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: directValue(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: internalReference(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: mappingString(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true
//@[none, gas, size] run-call: mappingStringErc7201(bytes32,bytes1) 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00 => true

contract StorageBytesHashing {
    struct Holder {
        bytes data;
    }

    mapping(uint256 => bytes) private values;
    mapping(uint256 => mapping(uint256 => bytes)) private nested;
    mapping(uint256 => string) private strings;
    Holder private holder;
    bytes private direct;

    function mappingEmpty() external returns (bool) {
        bytes memory expected = new bytes(0);
        values[0] = expected;
        return keccak256(values[0]) == keccak256(expected);
    }

    function mappingLong(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        values[0] = expected;
        return keccak256(values[0]) == keccak256(expected);
    }

    function mappingShort(bytes31 value) external returns (bool) {
        bytes memory expected = abi.encodePacked(value);
        values[1] = expected;
        return keccak256(values[1]) == keccak256(expected);
    }

    function mappingWord(bytes32 value) external returns (bool) {
        bytes memory expected = abi.encodePacked(value);
        values[2] = expected;
        return keccak256(values[2]) == keccak256(expected);
    }

    function mappingTwoWords(bytes32 first, bytes32 second, bytes1 tail)
        external
        returns (bool)
    {
        bytes memory expected = abi.encodePacked(first, second, tail);
        values[3] = expected;
        return keccak256(values[3]) == keccak256(expected);
    }

    function mappingReference(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        values[4] = expected;
        bytes storage value = values[4];
        return keccak256(value) == keccak256(expected);
    }

    function nestedMapping(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        nested[0][1] = expected;
        return keccak256(nested[0][1]) == keccak256(expected);
    }

    function structField(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        holder.data = expected;
        return keccak256(holder.data) == keccak256(expected);
    }

    function directValue(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        direct = expected;
        return keccak256(direct) == keccak256(expected);
    }

    function internalReference(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        values[5] = expected;
        return hashStorageBytes(values[5]) == keccak256(expected);
    }

    function hashStorageBytes(bytes storage value) internal pure returns (bytes32) {
        return keccak256(value);
    }

    function mappingString(bytes32 word, bytes1 tail) external returns (bool) {
        bytes memory expected = abi.encodePacked(word, tail);
        strings[0] = string(expected);
        return keccak256(bytes(strings[0])) == keccak256(expected);
    }

    function mappingStringErc7201(bytes32 word, bytes1 tail) external returns (bool) {
        string memory expected = string(abi.encodePacked(word, tail));
        strings[1] = expected;
        return erc7201(strings[1]) == erc7201(expected);
    }
}
