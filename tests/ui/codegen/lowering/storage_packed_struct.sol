//@ run-call: read => 1, 2, 3
//@ run-call: copyValues => 1, 2, 3
//@ run-call: raw => 0x0000000000000000000000000000000000000000000000000000000000000201
//@ run-call: write 7, 9, 11 => 7, 9, 11
//@ run-call: replace 8, 10, 12 => 8, 10, 12
//@ run-call: replaceArray => 5, 6, 7, 8
//@ run-call: arrayRead 3 => 4
//@ run-call: arrayWrite 2, 9 => 9
//@ run-call: mixedRead => 0x0000000000000000000000000000000000001234, true, 4660
//@ run-call: encodedRead => 0x11223344, -7
//@ run-call: encodedWrite 0xaabbccdd, -9 => 0xaabbccdd, -9

contract PackedStruct {
    struct S {
        uint8 a;
        uint8 b;
        uint256 c;
    }

    S private value;
    uint8[4] private packedArray;

    struct Mixed {
        address owner;
        bool enabled;
        uint16 count;
    }

    Mixed private mixed;

    struct Encoded {
        bytes4 tag;
        int8 signed;
    }

    Encoded private encoded;

    constructor() {
        value.a = 1;
        value.b = 2;
        value.c = 3;
        packedArray[0] = 1;
        packedArray[1] = 2;
        packedArray[2] = 3;
        packedArray[3] = 4;
        mixed.owner = address(0x1234);
        mixed.enabled = true;
        mixed.count = 0x1234;
        encoded.tag = 0x11223344;
        encoded.signed = -7;
    }

    function read() external view returns (uint8, uint8, uint256) {
        return (value.a, value.b, value.c);
    }

    function copyValues() external view returns (uint8, uint8, uint256) {
        S memory result = value;
        return (result.a, result.b, result.c);
    }

    function raw() external view returns (bytes32 result) {
        assembly {
            result := sload(0)
        }
    }

    function write(uint8 a, uint8 b, uint256 c)
        external
        returns (uint8, uint8, uint256)
    {
        value.a = a;
        value.b = b;
        value.c = c;
        return (value.a, value.b, value.c);
    }

    function replace(uint8 a, uint8 b, uint256 c)
        external
        returns (uint8, uint8, uint256)
    {
        S memory replacement = S(a, b, c);
        value = replacement;
        return (value.a, value.b, value.c);
    }

    function arrayRead(uint256 i) external view returns (uint8) {
        return packedArray[i];
    }

    function arrayWrite(uint256 i, uint8 x) external returns (uint8) {
        packedArray[i] = x;
        return packedArray[i];
    }

    function mixedRead() external view returns (address, bool, uint16) {
        return (mixed.owner, mixed.enabled, mixed.count);
    }

    function encodedRead() external view returns (bytes4, int8) {
        return (encoded.tag, encoded.signed);
    }

    function encodedWrite(bytes4 tag, int8 signed) external returns (bytes4, int8) {
        encoded.tag = tag;
        encoded.signed = signed;
        return (encoded.tag, encoded.signed);
    }

    function replaceArray() external returns (uint8, uint8, uint8, uint8) {
        uint8[4] memory replacement;
        replacement[0] = 5;
        replacement[1] = 6;
        replacement[2] = 7;
        replacement[3] = 8;
        packedArray = replacement;
        return (packedArray[0], packedArray[1], packedArray[2], packedArray[3]);
    }
}
