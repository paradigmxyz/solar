//@ run-call: read(uint256[3],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: read(uint256[3],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readDynamic(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamic(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readBytes(bytes) 0x010203 => 0x02
//@ run-call: readPair((uint8,uint8)) (7, 9) => 7
//@ run-call: readDynamicCalldata(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamicCalldata(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readBytesCalldata(bytes) 0x010203 => 0x02
//@ run-call: readDynamicPair((uint256,bytes)) (7, 0x010203) => 10
//@ run-call: readWordList((uint256[],uint256)) ([1, 2, 3], 7) => 10
//@ run-call: readWordList((uint256[],uint256)) ([], 7) => 7
//@ run-call: readSignedList((int256[],uint256)) ([1, 2], 7) => 9
//@ run-call: readNestedList((uint256[][])) ([[1, 2], [3]]) => 4
//@ run-call: readEnumPair((uint8,uint256)) (1, 9) => 10
//@ run-call-fail: readEnumPair((uint8,uint256)) (2, 9)
//@ run-call: readEnumArray(uint8[2]) [1, 0] => 1
//@ run-call-fail: readEnumArray(uint8[2]) [2, 0]
//@ run-call: readMode(uint8) 1 => 1
//@ run-call-fail: readMode(uint8) 2
//@ run-call: readNestedArray((uint256[2],uint8)) ([7, 8], 1) => 9
//@ run-call: readNestedBytes((bytes[2])) ([0x0102, 0x030405]) => 5
//@ run-call: readMixed(uint8,bytes) 1, 0x010203 => 4
//@ run-call: ConstructorAbiFixedArray::result(); constructor=[[[1, 2], [3, 4]], 5] => 8
//@ run-call: ConstructorAbiDynamic::result(); constructor=[[1, 2, 3], 0x010203] => 5
//@ run-call: ConstructorAbiDynamicWords::result(); constructor=[[1, 2, 3]] => 2
//@ run-call: ConstructorAbiBytes::result(); constructor=[0x010203] => 3
//@ run-call: ConstructorAbiStruct::result(); constructor=[(7, 0x010203)] => 10
//@ run-call: fixedArrayLengthSideEffect() => 2, 1

contract AbiFixedArray {
    uint256 private fixedArrayLengthCalls;

    struct Pair {
        uint8 first;
        uint8 second;
    }

    struct DynamicPair {
        uint256 value;
        bytes data;
    }

    struct WordList {
        uint256[] values;
        uint256 bias;
    }

    struct SignedList {
        int256[] values;
        uint256 bias;
    }

    struct NestedList {
        uint256[][] values;
    }

    enum Mode {
        Zero,
        One
    }

    struct EnumPair {
        Mode mode;
        uint256 value;
    }

    struct NestedArray {
        uint256[2] values;
        uint8 marker;
    }

    struct NestedBytes {
        bytes[2] values;
    }

    function makeFixedArray() internal returns (uint256[2] memory values) {
        fixedArrayLengthCalls += 1;
    }

    function fixedArrayLengthSideEffect() external returns (uint256, uint256) {
        return (makeFixedArray().length, fixedArrayLengthCalls);
    }

    function read(uint256[3] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamic(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamicCalldata(uint256[] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readBytes(bytes memory values) external pure returns (bytes1) {
        return values[1];
    }

    function readBytesCalldata(bytes calldata values) external pure returns (bytes1) {
        return values[1];
    }

    function readPair(Pair memory value) external pure returns (uint256) {
        return value.first;
    }

    function readDynamicPair(DynamicPair memory value) external pure returns (uint256) {
        return value.value + value.data.length;
    }

    function readWordList(WordList memory value) external pure returns (uint256) {
        return value.values.length + value.bias;
    }

    function readSignedList(SignedList memory value) external pure returns (uint256) {
        return value.values.length + value.bias;
    }

    function readNestedList(NestedList memory value) external pure returns (uint256) {
        return value.values.length + value.values[0].length;
    }

    function readEnumPair(EnumPair memory value) external pure returns (uint256) {
        return uint256(value.mode) + value.value;
    }

    function readEnumArray(Mode[2] memory values) external pure returns (uint256) {
        return uint256(values[0]);
    }

    function readMode(Mode mode) external pure returns (uint256) {
        return uint256(mode);
    }

    function readNestedArray(NestedArray memory value) external pure returns (uint256) {
        return value.values[1] + value.marker;
    }

    function readNestedBytes(NestedBytes memory value) external pure returns (uint256) {
        return value.values[0].length + value.values[1].length;
    }

    function readMixed(Mode mode, bytes memory data) external pure returns (uint256) {
        return uint256(mode) + data.length;
    }
}

contract ConstructorAbiFixedArray {
    uint256 public result;

    constructor(uint256[2][2] memory values, int256 bias) {
        result = values[1][0] + uint256(bias);
    }
}

contract ConstructorAbiDynamic {
    uint256 public result;

    constructor(uint256[] memory values, bytes memory data) {
        result = values[1] + data.length;
    }
}

contract ConstructorAbiDynamicWords {
    uint256 public result;

    constructor(uint256[] memory values) {
        result = values[1];
    }
}

contract ConstructorAbiBytes {
    uint256 public result;

    constructor(bytes memory data) {
        result = data.length;
    }
}

contract ConstructorAbiStruct {
    struct Value {
        uint256 value;
        bytes data;
    }

    uint256 public result;

    constructor(Value memory value) {
        result = value.value + value.data.length;
    }
}
