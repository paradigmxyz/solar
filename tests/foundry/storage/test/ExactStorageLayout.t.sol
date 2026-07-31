// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface StorageVm {
    function load(address target, bytes32 slot) external view returns (bytes32 value);
    function store(address target, bytes32 slot, bytes32 value) external;
}

contract ScalarLayoutBase {
    uint128 internal baseWide;
    uint32 internal baseSmall;
}

contract ScalarLayoutTarget is ScalarLayoutBase {
    enum Kind {
        Zero,
        One,
        Two
    }

    bytes3 private fixedBytes;
    uint8 private tiny;
    address private owner;
    bool private enabled;
    Kind private kind;
    function() external view returns (uint256) private callback;
    function(uint256) internal pure returns (uint256) private operation;
    uint8 private sentinel;

    function externalTarget() external pure returns (uint256) {
        return 77;
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function seed() external {
        baseWide = 0x112233445566778899aabbccddeeff00;
        baseSmall = 0x12345678;
        fixedBytes = hex"abcdef";
        tiny = 0x5a;
        owner = address(0x1234567890AbcdEF1234567890aBcdef12345678);
        enabled = true;
        kind = Kind.Two;
        callback = this.externalTarget;
        operation = increment;
        sentinel = 0x7f;
    }

    function updateTiny(uint8 value) external {
        tiny = value;
    }

    function clearOperation() external {
        delete operation;
    }

    function callStored() external view returns (uint256, uint256) {
        return (callback(), operation(41));
    }

    function callExternal() external view returns (uint256) {
        return callback();
    }
}

contract ArrayLayoutTarget {
    bytes3[12] private fixedValues;
    uint8 private afterFixed;
    bytes3[] private dynamicValues;
    uint8[] private byteValues;
    uint248[2] private wideValues;
    uint8 private afterWide;
    uint8[5] private shortTarget;
    uint8 private afterShortTarget;
    uint256[4] private wordTarget;

    function seed() external {
        bytes3[12] memory fixedCopy;
        fixedCopy[0] = hex"010203";
        fixedCopy[9] = hex"abcdef";
        fixedCopy[10] = hex"123456";
        fixedCopy[11] = hex"789abc";
        fixedValues = fixedCopy;
        afterFixed = 0x44;

        bytes3[] memory dynamicCopy = new bytes3[](11);
        dynamicCopy[0] = hex"040506";
        dynamicCopy[9] = hex"c0ffee";
        dynamicCopy[10] = hex"654321";
        dynamicValues = dynamicCopy;

        uint8[] memory byteCopy = new uint8[](35);
        byteCopy[0] = 1;
        byteCopy[31] = 32;
        byteCopy[32] = 33;
        byteCopy[34] = 35;
        byteValues = byteCopy;

        wideValues = [uint248(0x1234), uint248(0xabcdef)];
        afterWide = 0x55;
    }

    function shrinkDynamic() external {
        bytes3[] memory values = new bytes3[](1);
        values[0] = hex"112233";
        dynamicValues = values;

        uint8[] memory bytesCopy = new uint8[](1);
        bytesCopy[0] = 9;
        byteValues = bytesCopy;
    }

    function clearAll() external {
        delete fixedValues;
        delete dynamicValues;
        delete byteValues;
        delete wideValues;
    }

    function copyShorterFixedArrays() external {
        uint8[3] memory packedSource = [uint8(0x11), uint8(0x22), uint8(0x33)];
        shortTarget = packedSource;
        afterShortTarget = 0x66;
        uint256[2] memory wordSource = [uint256(0x1234), uint256(0x5678)];
        wordTarget = wordSource;
    }

    function resetByteBoundary() external {
        byteValues = new uint8[](32);
    }

    function pushByte(uint8 value) external {
        byteValues.push(value);
    }

    function popByte() external {
        byteValues.pop();
    }

    function pushEmptyByte() external returns (uint8) {
        return byteValues.push();
    }

    function resetFixedBytesBoundary() external {
        dynamicValues = new bytes3[](10);
    }

    function pushFixedBytes() external {
        dynamicValues.push(hex"abcdef");
    }

    function popFixedBytes() external {
        dynamicValues.pop();
    }
}

contract MappingLayoutTarget {
    mapping(uint256 => bytes3) private fixedValues;
    mapping(uint256 => int8) private signedValues;
    mapping(address => mapping(uint256 => bool)) private nestedValues;

    function seed(uint256 key, address account) external {
        fixedValues[key] = hex"abcdef";
        signedValues[key] = -7;
        nestedValues[account][key] = true;
    }

    function clear(uint256 key, address account) external {
        delete fixedValues[key];
        delete signedValues[key];
        delete nestedValues[account][key];
    }
}

contract MappingArrayLayoutTarget {
    mapping(uint256 => uint8)[] private values;

    function seed(uint256 key) external {
        values.push();
        values[0][key] = 0x44;
    }

    function popAndPush() external {
        values.pop();
        values.push();
    }
}

contract NestedArrayLayoutTarget {
    uint8[][] private values;

    function seed() external {
        values.push();
        values[0].push(0x11);
        values[0].push(0x22);
        values.push();
        values[1].push(0x33);
    }

    function popOuter() external {
        values.pop();
    }

    function clearAll() external {
        delete values;
    }
}

contract AggregateAssignmentTarget {
    struct Record {
        uint8 flag;
        uint8[] items;
    }

    Record private sourceRecord;
    Record private targetRecord;
    uint8[] private sourceDynamic;
    uint8[] private targetDynamic;
    uint8[3] private sourceFixed;
    uint8[5] private targetFixed;
    mapping(uint256 => uint8[]) private mapped;
    uint8[] private fixedToDynamic;
    uint8[5][4] private nestedTarget;

    function seed() external {
        sourceRecord.flag = 0x11;
        sourceRecord.items.push(0x22);
        sourceRecord.items.push(0x33);
        targetRecord = sourceRecord;

        uint8[] memory memberCopy = new uint8[](2);
        memberCopy[0] = 0x44;
        memberCopy[1] = 0x55;
        targetRecord.items = memberCopy;

        sourceDynamic.push(0x66);
        sourceDynamic.push(0x77);
        targetDynamic = sourceDynamic;

        sourceFixed = [uint8(0x88), uint8(0x99), uint8(0xaa)];
        targetFixed = sourceFixed;
        mapped[7] = memberCopy;

        uint8[2] memory fixedMemory = [uint8(0xbb), uint8(0xcc)];
        fixedToDynamic = fixedMemory;

        uint8[3][2] memory nestedMemory;
        nestedMemory[0] = [uint8(1), uint8(2), uint8(3)];
        nestedMemory[1] = [uint8(4), uint8(5), uint8(6)];
        nestedTarget = nestedMemory;
    }
}

contract WideDynamicLayoutTarget {
    uint248[] private values;

    function seed() external {
        uint248[] memory copy = new uint248[](2);
        copy[0] = 0x1234;
        copy[1] = 0x5678;
        values = copy;
    }

    function shrink() external {
        uint248[] memory copy = new uint248[](1);
        copy[0] = 0xabcd;
        values = copy;
    }

    function clear() external {
        delete values;
    }
}

contract StructLayoutTarget {
    struct Record {
        uint8 a;
        uint16 b;
        bytes3 c;
        uint256 wide;
        uint8 d;
        uint8[] items;
        mapping(uint256 => uint8) mapped;
        uint8 tail;
    }

    Record private record;
    uint8 private afterRecord;

    function seed() external {
        record.a = 1;
        record.b = 0x0203;
        record.c = hex"abcdef";
        record.wide = 0x123456;
        record.d = 4;
        uint8[] memory values = new uint8[](35);
        values[0] = 5;
        values[31] = 32;
        values[32] = 33;
        values[34] = 35;
        record.items = values;
        record.mapped[7] = 9;
        record.tail = 6;
        afterRecord = 7;
    }

    function clearRecord() external {
        delete record;
    }
}

contract ScalarDeleteLayoutTarget {
    struct Packed {
        uint48 value;
    }

    Packed private packed;
    bool private sourceBool;
    bool private targetBool;
    bool[1] private memoryBoolTarget;

    function clearPacked() external {
        delete packed;
    }

    function copyBool() external {
        targetBool = sourceBool;
    }

    function copyDirtyMemoryBool() external {
        bool[1] memory values;
        assembly {
            mstore(values, 2)
        }
        memoryBoolTarget = values;
    }
}

contract StorageToMemoryCleanupTarget {
    enum Kind {
        Zero,
        One
    }

    struct Plain {
        bool flag;
        Kind kind;
    }

    struct Deep {
        bool flag;
        uint8[] values;
        Kind kind;
    }

    bool[1] private fixedBools;
    bool[] private dynamicBools;
    Plain private plain;
    Deep private deep;
    Kind[1] private fixedKinds;
    Kind[] private dynamicKinds;

    function fixedBoolValue() external view returns (uint256 value) {
        bool[1] memory values = fixedBools;
        assembly {
            value := mload(values)
        }
    }

    function dynamicBoolValue() external view returns (uint256 value) {
        bool[] memory values = dynamicBools;
        assembly {
            value := mload(add(values, 32))
        }
    }

    function plainValues() external view returns (uint256 flag, uint256 kind) {
        Plain memory value = plain;
        assembly {
            flag := mload(value)
            kind := mload(add(value, 32))
        }
    }

    function deepValues() external view returns (uint256 flag, uint256 kind) {
        Deep memory value = deep;
        assembly {
            flag := mload(value)
            kind := mload(add(value, 64))
        }
    }

    function copyFixedKind() external view returns (Kind[1] memory) {
        return fixedKinds;
    }

    function copyDynamicKind() external view returns (Kind[] memory) {
        return dynamicKinds;
    }
}

contract BytesLayoutTarget {
    bytes private data;

    function setShort() external {
        data = hex"616263";
    }

    function setLong() external {
        data =
            hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627";
    }

    function setLength(uint256 length) external {
        data = new bytes(length);
    }

    function pushEmpty() external returns (bytes1) {
        return data.push();
    }

    function setDirtyLongTail() external {
        bytes memory value = new bytes(33);
        assembly {
            mstore(add(value, 64), or(shl(248, 0x11), sub(shl(248, 1), 1)))
        }
        data = value;
    }
}

contract CalldataArrayLayoutTarget {
    uint8[] private values;

    function set(uint8[] calldata newValues) external {
        values = newValues;
    }
}

contract DifferentPackingLayoutTarget {
    bytes8[] private source;
    bytes10[] private target;

    function copy() external {
        source = new bytes8[](9);
        for (uint256 i = 0; i < source.length; ++i) {
            source[i] = bytes8(uint64(i));
        }
        target = source;
    }
}

contract TupleCleanupLayoutTarget {
    uint32[] private values;

    constructor() {
        values.push();
        values.push();
    }

    function clean() external {
        (values[1], values) = (4, [uint32(0)]);
        values = [uint32(0)];
        values.push();
    }
}

contract StorageAliasLayoutTarget {
    enum Kind {
        Zero,
        One
    }

    mapping(uint256 => uint8[]) private arrays;
    bytes private data;
    bool[] private sourceBools;
    bool[] private targetBools;
    Kind[] private kinds;

    function selfAssignArray(uint256 key) external {
        uint8[] storage value = arrays[key];
        value = value;
    }

    function selfAssignBytes() external {
        data = data;
    }

    function copyBools() external {
        targetBools = sourceBools;
    }

    function selfAssignKinds() external {
        kinds = kinds;
    }
}

contract ShiftedLayoutTarget layout at 42 {
    uint128 private first;
    uint64 private second;
    uint256 private third;

    function seed() external {
        first = 0x112233445566778899aabbccddeeff00;
        second = 0x0102030405060708;
        third = 0xabcdef;
    }
}

contract InitializedLayoutBase {
    uint128 private baseValue = 0x112233445566778899aabbccddeeff00;
}

contract InitializedLayoutTarget is InitializedLayoutBase {
    struct Packed {
        uint8 first;
        uint16 second;
    }

    uint32 private derivedValue = 0x12345678;
    uint8[3] private fixedValues = [uint8(1), uint8(2), uint8(3)];
    uint8[] private dynamicValues = [uint8(4), uint8(5), uint8(6)];
    bytes private byteValues = "abc";
    Packed private packed = Packed(0x44, 0x5566);
}

contract ExactStorageLayoutTest {
    StorageVm private constant vm =
        StorageVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testScalarAndInheritanceSlots() public {
        ScalarLayoutTarget target = new ScalarLayoutTarget();
        target.seed();

        uint256 slot0 = uint256(vm.load(address(target), bytes32(uint256(0))));
        uint256 expected0 = uint256(0x112233445566778899aabbccddeeff00)
            | (uint256(0x12345678) << 128) | (uint256(0xabcdef) << 160)
            | (uint256(0x5a) << 184);
        require(slot0 == expected0, "scalar slot 0");

        uint256 slot1 = uint256(vm.load(address(target), bytes32(uint256(1))));
        uint256 expected1 = uint256(uint160(0x1234567890AbcdEF1234567890aBcdef12345678))
            | (uint256(1) << 160) | (uint256(2) << 168);
        require(slot1 == expected1, "scalar slot 1");

        uint256 slot2 = uint256(vm.load(address(target), bytes32(uint256(2))));
        uint256 externalValue =
            (uint256(uint160(address(target))) << 32) | uint32(target.externalTarget.selector);
        require(uint192(slot2) == uint192(externalValue), "external function bytes");
        require(slot2 >> 192 != 0, "internal function bytes");
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 0x7f, "sentinel slot");

        target.updateTiny(0xa5);
        require(
            uint256(vm.load(address(target), bytes32(uint256(0))))
                == ((expected0 & ~(uint256(0xff) << 184)) | (uint256(0xa5) << 184)),
            "packed neighbor preservation"
        );
        (uint256 externalResult, uint256 internalResult) = target.callStored();
        require(externalResult == 77 && internalResult == 42, "stored function calls");

        target.clearOperation();
        require(
            uint192(uint256(vm.load(address(target), bytes32(uint256(2)))))
                == uint192(externalValue),
            "keep external function"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(2)))) >> 192 == 0,
            "clear internal function"
        );
        require(target.callExternal() == 77, "external function after delete");
    }

    function testFixedAndDynamicArraySlots() public {
        ArrayLayoutTarget target = new ArrayLayoutTarget();
        vm.store(address(target), bytes32(uint256(0)), bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(1)), bytes32(type(uint256).max));
        bytes32 dynamicBase = keccak256(abi.encode(uint256(3)));
        bytes32 byteBase = keccak256(abi.encode(uint256(4)));
        vm.store(address(target), dynamicBase, bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(dynamicBase) + 1), bytes32(type(uint256).max));
        vm.store(address(target), byteBase, bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(byteBase) + 1), bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(5)), bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(6)), bytes32(type(uint256).max));

        target.seed();

        uint256 fixed0 = uint256(0x010203) | (uint256(0xabcdef) << 216);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == fixed0, "fixed slot 0");
        require(
            uint256(vm.load(address(target), bytes32(uint256(1))))
                == (uint256(0x123456) | (uint256(0x789abc) << 24)),
            "fixed slot 1"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(2)))) == 0x44, "after fixed");

        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 11, "dynamic length");
        require(
            uint256(vm.load(address(target), dynamicBase))
                == (uint256(0x040506) | (uint256(0xc0ffee) << 216)),
            "dynamic slot 0"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(dynamicBase) + 1))) == 0x654321,
            "dynamic slot 1"
        );

        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 35, "byte length");
        require(
            uint256(vm.load(address(target), byteBase)) == (uint256(1) | (uint256(32) << 248)),
            "byte slot 0"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(byteBase) + 1)))
                == (uint256(33) | (uint256(35) << 16)),
            "byte slot 1"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(5)))) == 0x1234, "wide slot 0");
        require(uint256(vm.load(address(target), bytes32(uint256(6)))) == 0xabcdef, "wide slot 1");
        require(uint256(vm.load(address(target), bytes32(uint256(7)))) == 0x55, "after wide");

        target.shrinkDynamic();
        require(uint256(vm.load(address(target), dynamicBase)) == 0x112233, "shrunk dynamic");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dynamicBase) + 1))) == 0,
            "cleared dynamic tail"
        );
        require(uint256(vm.load(address(target), byteBase)) == 9, "shrunk bytes");
        require(
            uint256(vm.load(address(target), bytes32(uint256(byteBase) + 1))) == 0,
            "cleared byte tail"
        );

        target.clearAll();
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 0, "clear fixed 0");
        require(uint256(vm.load(address(target), bytes32(uint256(1)))) == 0, "clear fixed 1");
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 0, "clear dynamic head");
        require(uint256(vm.load(address(target), dynamicBase)) == 0, "clear dynamic data");
        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 0, "clear byte head");
        require(uint256(vm.load(address(target), byteBase)) == 0, "clear byte data");
        require(uint256(vm.load(address(target), bytes32(uint256(5)))) == 0, "clear wide 0");
        require(uint256(vm.load(address(target), bytes32(uint256(6)))) == 0, "clear wide 1");
    }

    function testFixedArrayConversionsAndPackedBoundaries() public {
        ArrayLayoutTarget target = new ArrayLayoutTarget();
        for (uint256 slot = 8; slot <= 13; ++slot) {
            vm.store(address(target), bytes32(slot), bytes32(type(uint256).max));
        }
        target.copyShorterFixedArrays();

        require(
            uint256(vm.load(address(target), bytes32(uint256(8))))
                == (uint256(0x11) | (uint256(0x22) << 8) | (uint256(0x33) << 16)),
            "short packed source"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(9))))
                == ((type(uint256).max << 8) | uint256(0x66)),
            "after short target"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(10)))) == 0x1234, "short word 0");
        require(uint256(vm.load(address(target), bytes32(uint256(11)))) == 0x5678, "short word 1");
        require(uint256(vm.load(address(target), bytes32(uint256(12)))) == 0, "short word tail 0");
        require(uint256(vm.load(address(target), bytes32(uint256(13)))) == 0, "short word tail 1");

        bytes32 byteBase = keccak256(abi.encode(uint256(4)));
        target.resetByteBoundary();
        vm.store(address(target), byteBase, bytes32(type(uint256).max));
        vm.store(
            address(target),
            bytes32(uint256(byteBase) + 1),
            bytes32(type(uint256).max)
        );
        target.pushByte(0x7a);
        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 33, "byte push length");
        require(
            uint256(vm.load(address(target), bytes32(uint256(byteBase) + 1)))
                == ((type(uint256).max << 8) | uint256(0x7a)),
            "byte push boundary"
        );
        target.popByte();
        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 32, "byte pop length");
        require(
            uint256(vm.load(address(target), bytes32(uint256(byteBase) + 1)))
                == (type(uint256).max << 8),
            "byte pop boundary"
        );
        vm.store(
            address(target),
            bytes32(uint256(byteBase) + 1),
            bytes32(type(uint256).max)
        );
        require(target.pushEmptyByte() == type(uint8).max, "empty push value");
        require(
            uint256(vm.load(address(target), bytes32(uint256(byteBase) + 1)))
                == type(uint256).max,
            "empty push keeps storage"
        );

        bytes32 fixedBase = keccak256(abi.encode(uint256(3)));
        target.resetFixedBytesBoundary();
        vm.store(address(target), fixedBase, bytes32(type(uint256).max));
        vm.store(
            address(target),
            bytes32(uint256(fixedBase) + 1),
            bytes32(type(uint256).max)
        );
        target.pushFixedBytes();
        require(
            uint256(vm.load(address(target), bytes32(uint256(3)))) == 11,
            "bytes3 push length"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(fixedBase) + 1)))
                == ((type(uint256).max << 24) | uint256(0xabcdef)),
            "bytes3 push boundary"
        );
        target.popFixedBytes();
        require(
            uint256(vm.load(address(target), bytes32(uint256(3)))) == 10,
            "bytes3 pop length"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(fixedBase) + 1)))
                == (type(uint256).max << 24),
            "bytes3 pop boundary"
        );
    }

    function testMappingSlotsAndPadding() public {
        MappingLayoutTarget target = new MappingLayoutTarget();
        uint256 key = 7;
        address account = address(0x1234);
        bytes32 fixedSlot = keccak256(abi.encode(key, uint256(0)));
        bytes32 signedSlot = keccak256(abi.encode(key, uint256(1)));
        bytes32 nestedOuter = keccak256(abi.encode(account, uint256(2)));
        bytes32 nestedSlot = keccak256(abi.encode(key, nestedOuter));
        vm.store(address(target), fixedSlot, bytes32(type(uint256).max));
        vm.store(address(target), signedSlot, bytes32(type(uint256).max));
        vm.store(address(target), nestedSlot, bytes32(type(uint256).max));

        target.seed(key, account);

        require(
            uint256(vm.load(address(target), fixedSlot))
                == ((type(uint256).max << 24) | uint256(0xabcdef)),
            "fixed mapping"
        );
        require(
            uint256(vm.load(address(target), signedSlot))
                == ((type(uint256).max << 8) | uint256(0xf9)),
            "signed mapping"
        );
        require(
            uint256(vm.load(address(target), nestedSlot))
                == ((type(uint256).max << 8) | uint256(1)),
            "nested mapping"
        );

        target.clear(key, account);
        require(
            uint256(vm.load(address(target), fixedSlot)) == (type(uint256).max << 24),
            "clear fixed mapping"
        );
        require(
            uint256(vm.load(address(target), signedSlot)) == (type(uint256).max << 8),
            "clear signed mapping"
        );
        require(
            uint256(vm.load(address(target), nestedSlot)) == (type(uint256).max << 8),
            "clear nested mapping"
        );
    }

    function testMappingArrayPersistenceAndNestedArrays() public {
        uint256 key = 7;
        MappingArrayLayoutTarget mappingTarget = new MappingArrayLayoutTarget();
        mappingTarget.seed(key);
        bytes32 outerData = keccak256(abi.encode(uint256(0)));
        bytes32 mappedSlot = keccak256(abi.encode(key, uint256(outerData)));
        require(
            uint256(vm.load(address(mappingTarget), bytes32(uint256(0)))) == 1,
            "mapping array length"
        );
        require(
            uint256(vm.load(address(mappingTarget), mappedSlot)) == 0x44,
            "mapping array value"
        );
        mappingTarget.popAndPush();
        require(
            uint256(vm.load(address(mappingTarget), bytes32(uint256(0)))) == 1,
            "mapping array repush length"
        );
        require(
            uint256(vm.load(address(mappingTarget), mappedSlot)) == 0x44,
            "mapping array persistence"
        );

        NestedArrayLayoutTarget nestedTarget = new NestedArrayLayoutTarget();
        nestedTarget.seed();
        bytes32 nestedOuter = keccak256(abi.encode(uint256(0)));
        bytes32 inner0 = keccak256(abi.encode(uint256(nestedOuter)));
        bytes32 inner1 = keccak256(abi.encode(uint256(nestedOuter) + 1));
        require(
            uint256(vm.load(address(nestedTarget), bytes32(uint256(0)))) == 2,
            "nested outer length"
        );
        require(
            uint256(vm.load(address(nestedTarget), nestedOuter)) == 2,
            "nested inner 0 length"
        );
        require(
            uint256(vm.load(address(nestedTarget), bytes32(uint256(nestedOuter) + 1))) == 1,
            "nested inner 1 length"
        );
        require(
            uint256(vm.load(address(nestedTarget), inner0))
                == (uint256(0x11) | (uint256(0x22) << 8)),
            "nested inner 0 data"
        );
        require(
            uint256(vm.load(address(nestedTarget), inner1)) == 0x33,
            "nested inner 1 data"
        );

        nestedTarget.popOuter();
        require(
            uint256(vm.load(address(nestedTarget), bytes32(uint256(0)))) == 1,
            "nested pop outer length"
        );
        require(
            uint256(vm.load(address(nestedTarget), bytes32(uint256(nestedOuter) + 1))) == 0,
            "nested pop inner head"
        );
        require(uint256(vm.load(address(nestedTarget), inner1)) == 0, "nested pop inner data");
        nestedTarget.clearAll();
        require(
            uint256(vm.load(address(nestedTarget), bytes32(uint256(0)))) == 0,
            "nested clear outer length"
        );
        require(uint256(vm.load(address(nestedTarget), nestedOuter)) == 0, "nested clear inner head");
        require(uint256(vm.load(address(nestedTarget), inner0)) == 0, "nested clear inner data");
    }

    function testStructSlotsAndRecursiveDelete() public {
        StructLayoutTarget target = new StructLayoutTarget();
        target.seed();

        require(
            uint256(vm.load(address(target), bytes32(uint256(0))))
                == (uint256(1) | (uint256(0x0203) << 8) | (uint256(0xabcdef) << 24)),
            "struct packed head"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(1)))) == 0x123456,
            "struct wide"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(2)))) == 4, "struct d");
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 35, "struct array length");
        bytes32 dataBase = keccak256(abi.encode(uint256(3)));
        require(
            uint256(vm.load(address(target), dataBase)) == (uint256(5) | (uint256(32) << 248)),
            "struct array 0"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1)))
                == (uint256(33) | (uint256(35) << 16)),
            "struct array 1"
        );
        bytes32 mappedSlot = keccak256(abi.encode(uint256(7), uint256(4)));
        require(uint256(vm.load(address(target), mappedSlot)) == 9, "struct mapping");
        require(uint256(vm.load(address(target), bytes32(uint256(5)))) == 6, "struct tail");
        require(uint256(vm.load(address(target), bytes32(uint256(6)))) == 7, "after struct");

        vm.store(address(target), bytes32(uint256(0)), bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(2)), bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(4)), bytes32(uint256(0xfeed)));
        vm.store(address(target), bytes32(uint256(5)), bytes32(type(uint256).max));
        target.clearRecord();

        require(
            uint256(vm.load(address(target), bytes32(uint256(0)))) == (type(uint256).max << 48),
            "delete head"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(1)))) == 0, "delete wide");
        require(
            uint256(vm.load(address(target), bytes32(uint256(2)))) == (type(uint256).max << 8),
            "delete d"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 0, "delete array head");
        require(uint256(vm.load(address(target), dataBase)) == 0, "delete array data 0");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1))) == 0,
            "delete array data 1"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 0xfeed, "mapping base");
        require(uint256(vm.load(address(target), mappedSlot)) == 9, "mapping value");
        require(
            uint256(vm.load(address(target), bytes32(uint256(5)))) == (type(uint256).max << 8),
            "delete tail"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(6)))) == 7, "keep next state");
    }

    function testScalarStructDeleteAndBoolCleanup() public {
        ScalarDeleteLayoutTarget target = new ScalarDeleteLayoutTarget();
        vm.store(address(target), bytes32(uint256(0)), bytes32(type(uint256).max));
        target.clearPacked();
        require(
            uint256(vm.load(address(target), bytes32(uint256(0)))) == (type(uint256).max << 48),
            "scalar struct padding"
        );

        vm.store(address(target), bytes32(uint256(1)), bytes32(uint256(2)));
        target.copyBool();
        require(
            uint256(vm.load(address(target), bytes32(uint256(1)))) == (uint256(2) | (uint256(1) << 8)),
            "bool cleanup"
        );
        target.copyDirtyMemoryBool();
        require(
            uint256(vm.load(address(target), bytes32(uint256(2)))) == 1,
            "memory bool cleanup"
        );
    }

    function testStorageToMemoryBoolCleanup() public {
        StorageToMemoryCleanupTarget target = new StorageToMemoryCleanupTarget();

        vm.store(address(target), bytes32(uint256(0)), bytes32(uint256(2)));
        require(target.fixedBoolValue() == 1, "fixed bool cleanup");

        bytes32 dynamicBoolData = keccak256(abi.encode(uint256(1)));
        vm.store(address(target), bytes32(uint256(1)), bytes32(uint256(1)));
        vm.store(address(target), dynamicBoolData, bytes32(uint256(2)));
        require(target.dynamicBoolValue() == 1, "dynamic bool cleanup");

        vm.store(
            address(target),
            bytes32(uint256(2)),
            bytes32(uint256(2) | (uint256(1) << 8))
        );
        (uint256 plainFlag, uint256 plainKind) = target.plainValues();
        require(plainFlag == 1 && plainKind == 1, "plain cleanup");

        vm.store(address(target), bytes32(uint256(3)), bytes32(uint256(2)));
        vm.store(address(target), bytes32(uint256(5)), bytes32(uint256(1)));
        (uint256 deepFlag, uint256 deepKind) = target.deepValues();
        require(deepFlag == 1 && deepKind == 1, "deep cleanup");
    }

    function testStorageToMemoryEnumCleanup() public {
        StorageToMemoryCleanupTarget target = new StorageToMemoryCleanupTarget();
        bytes memory panic = abi.encodeWithSelector(bytes4(0x4e487b71), uint256(0x21));
        vm.store(address(target), bytes32(uint256(6)), bytes32(uint256(2)));
        (bool fixedOk, bytes memory fixedReason) =
            address(target).call(abi.encodeCall(target.copyFixedKind, ()));
        require(!fixedOk && keccak256(fixedReason) == keccak256(panic), "fixed enum cleanup");

        bytes32 dynamicKindData = keccak256(abi.encode(uint256(7)));
        vm.store(address(target), bytes32(uint256(7)), bytes32(uint256(1)));
        vm.store(address(target), dynamicKindData, bytes32(uint256(2)));
        (bool dynamicOk, bytes memory dynamicReason) =
            address(target).call(abi.encodeCall(target.copyDynamicKind, ()));
        require(
            !dynamicOk && keccak256(dynamicReason) == keccak256(panic),
            "dynamic enum cleanup"
        );

        vm.store(
            address(target),
            bytes32(uint256(2)),
            bytes32(uint256(1) | (uint256(2) << 8))
        );
        (bool plainOk, bytes memory plainReason) =
            address(target).call(abi.encodeCall(target.plainValues, ()));
        require(!plainOk && keccak256(plainReason) == keccak256(panic), "plain enum cleanup");

        vm.store(address(target), bytes32(uint256(5)), bytes32(uint256(2)));
        (bool deepOk, bytes memory deepReason) =
            address(target).call(abi.encodeCall(target.deepValues, ()));
        require(!deepOk && keccak256(deepReason) == keccak256(panic), "deep enum cleanup");
    }

    function testAggregateAssignments() public {
        AggregateAssignmentTarget target = new AggregateAssignmentTarget();
        vm.store(address(target), bytes32(uint256(7)), bytes32(type(uint256).max));
        target.seed();

        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 0x11, "source record");
        require(uint256(vm.load(address(target), bytes32(uint256(1)))) == 2, "source items length");
        bytes32 sourceItems = keccak256(abi.encode(uint256(1)));
        require(
            uint256(vm.load(address(target), sourceItems))
                == (uint256(0x22) | (uint256(0x33) << 8)),
            "source items"
        );

        require(uint256(vm.load(address(target), bytes32(uint256(2)))) == 0x11, "target record");
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 2, "target items length");
        bytes32 targetItems = keccak256(abi.encode(uint256(3)));
        require(
            uint256(vm.load(address(target), targetItems))
                == (uint256(0x44) | (uint256(0x55) << 8)),
            "target items"
        );

        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 2, "source dyn length");
        require(uint256(vm.load(address(target), bytes32(uint256(5)))) == 2, "target dyn length");
        bytes32 sourceDynamic = keccak256(abi.encode(uint256(4)));
        bytes32 targetDynamic = keccak256(abi.encode(uint256(5)));
        uint256 dynamicWord = uint256(0x66) | (uint256(0x77) << 8);
        require(
            uint256(vm.load(address(target), sourceDynamic)) == dynamicWord,
            "source dynamic"
        );
        require(
            uint256(vm.load(address(target), targetDynamic)) == dynamicWord,
            "target dynamic"
        );

        require(
            uint256(vm.load(address(target), bytes32(uint256(6))))
                == (uint256(0x88) | (uint256(0x99) << 8) | (uint256(0xaa) << 16)),
            "source fixed"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(7))))
                == (uint256(0x88) | (uint256(0x99) << 8) | (uint256(0xaa) << 16)),
            "target fixed"
        );

        bytes32 mappedHead = keccak256(abi.encode(uint256(7), uint256(8)));
        bytes32 mappedData = keccak256(abi.encode(mappedHead));
        require(uint256(vm.load(address(target), mappedHead)) == 2, "mapped length");
        require(
            uint256(vm.load(address(target), mappedData))
                == (uint256(0x44) | (uint256(0x55) << 8)),
            "mapped data"
        );

        require(
            uint256(vm.load(address(target), bytes32(uint256(9)))) == 2,
            "fixed to dynamic length"
        );
        bytes32 fixedToDynamic = keccak256(abi.encode(uint256(9)));
        require(
            uint256(vm.load(address(target), fixedToDynamic))
                == (uint256(0xbb) | (uint256(0xcc) << 8)),
            "fixed to dynamic data"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(10))))
                == (uint256(1) | (uint256(2) << 8) | (uint256(3) << 16)),
            "nested fixed 0"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(11))))
                == (uint256(4) | (uint256(5) << 8) | (uint256(6) << 16)),
            "nested fixed 1"
        );
        require(uint256(vm.load(address(target), bytes32(uint256(12)))) == 0, "nested fixed tail 0");
        require(uint256(vm.load(address(target), bytes32(uint256(13)))) == 0, "nested fixed tail 1");
    }

    function testWideDynamicArrayPadding() public {
        WideDynamicLayoutTarget target = new WideDynamicLayoutTarget();
        bytes32 data = keccak256(abi.encode(uint256(0)));
        vm.store(address(target), data, bytes32(type(uint256).max));
        vm.store(address(target), bytes32(uint256(data) + 1), bytes32(type(uint256).max));
        target.seed();
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 2, "wide dyn length");
        require(
            uint256(vm.load(address(target), data)) == 0x1234,
            "wide dyn padding"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(data) + 1))) == 0x5678,
            "wide dyn second"
        );
        vm.store(address(target), bytes32(uint256(data) + 1), bytes32(type(uint256).max));
        target.shrink();
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 1, "wide shrink length");
        require(
            uint256(vm.load(address(target), data)) == 0xabcd,
            "wide shrink value"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(data) + 1))) == 0,
            "wide shrink tail"
        );
        target.clear();
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 0, "wide dyn clear head");
        require(uint256(vm.load(address(target), data)) == 0, "wide dyn clear data");
    }

    function testBytesShortLongAndShrinkSlots() public {
        BytesLayoutTarget target = new BytesLayoutTarget();
        target.setShort();
        require(
            uint256(vm.load(address(target), bytes32(uint256(0))))
                == (uint256(bytes32("abc")) | uint256(6)),
            "short bytes"
        );

        target.setLong();
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 81, "long head");
        bytes32 dataBase = keccak256(abi.encode(uint256(0)));
        require(
            vm.load(address(target), dataBase)
                == hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            "long data 0"
        );
        require(
            vm.load(address(target), bytes32(uint256(dataBase) + 1))
                == hex"2021222324252627000000000000000000000000000000000000000000000000",
            "long data 1"
        );

        target.setShort();
        require(
            uint256(vm.load(address(target), bytes32(uint256(0))))
                == (uint256(bytes32("abc")) | uint256(6)),
            "short after long"
        );
        require(uint256(vm.load(address(target), dataBase)) == 0, "clear long data 0");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1))) == 0,
            "clear long data 1"
        );

        vm.store(address(target), dataBase, bytes32(type(uint256).max));
        vm.store(
            address(target),
            bytes32(uint256(dataBase) + 1),
            bytes32(type(uint256).max)
        );
        target.setLength(31);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 62, "bytes length 31");
        require(uint256(vm.load(address(target), dataBase)) == type(uint256).max, "short data unused");

        target.setLength(32);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 65, "bytes length 32");
        require(uint256(vm.load(address(target), dataBase)) == 0, "bytes data 32");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1)))
                == type(uint256).max,
            "bytes second word unused"
        );

        target.setLength(33);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 67, "bytes length 33");
        require(uint256(vm.load(address(target), dataBase)) == 0, "bytes data 33 first");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1))) == 0,
            "bytes data 33 second"
        );

        target.setLength(31);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 62, "bytes shrink to 31");
        require(uint256(vm.load(address(target), dataBase)) == 0, "clear boundary data 0");
        require(
            uint256(vm.load(address(target), bytes32(uint256(dataBase) + 1))) == 0,
            "clear boundary data 1"
        );

        BytesLayoutTarget boundaryTarget = new BytesLayoutTarget();
        boundaryTarget.setLength(31);
        require(boundaryTarget.pushEmpty() == bytes1(0), "boundary push value");
        require(
            uint256(vm.load(address(boundaryTarget), bytes32(uint256(0)))) == 65,
            "boundary push head"
        );
        require(
            uint256(vm.load(address(boundaryTarget), keccak256(abi.encode(uint256(0))))) == 0,
            "boundary push data"
        );

        BytesLayoutTarget dirtyTarget = new BytesLayoutTarget();
        vm.store(
            address(dirtyTarget),
            bytes32(uint256(0)),
            bytes32(uint256(0xaa) << 248)
        );
        require(dirtyTarget.pushEmpty() == bytes1(0xaa), "empty bytes push value");
        require(
            uint256(vm.load(address(dirtyTarget), bytes32(uint256(0))))
                == ((uint256(0xaa) << 248) | uint256(2)),
            "empty bytes push storage"
        );
        dirtyTarget.setDirtyLongTail();
        bytes32 dirtyDataBase = keccak256(abi.encode(uint256(0)));
        require(
            uint256(vm.load(address(dirtyTarget), bytes32(uint256(dirtyDataBase) + 1)))
                == (uint256(0x11) << 248),
            "long bytes tail cleanup"
        );
    }

    function testCalldataPackedArraySlots() public {
        CalldataArrayLayoutTarget target = new CalldataArrayLayoutTarget();
        uint8[] memory values = new uint8[](35);
        values[0] = 1;
        values[31] = 32;
        values[32] = 33;
        values[34] = 35;
        target.set(values);

        bytes32 data = keccak256(abi.encode(uint256(0)));
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 35, "calldata length");
        require(
            uint256(vm.load(address(target), data)) == (uint256(1) | (uint256(32) << 248)),
            "calldata first slot"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(data) + 1)))
                == (uint256(33) | (uint256(35) << 16)),
            "calldata second slot"
        );

        uint8[] memory shorter = new uint8[](1);
        shorter[0] = 9;
        target.set(shorter);
        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 1, "calldata shrink");
        require(uint256(vm.load(address(target), data)) == 9, "calldata shrink first");
        require(
            uint256(vm.load(address(target), bytes32(uint256(data) + 1))) == 0,
            "calldata shrink tail"
        );
    }

    function testDifferentPackingAndTupleCleanupSlots() public {
        DifferentPackingLayoutTarget packing = new DifferentPackingLayoutTarget();
        bytes32 targetData = keccak256(abi.encode(uint256(1)));
        vm.store(address(packing), bytes32(uint256(1)), bytes32(uint256(12)));
        vm.store(
            address(packing),
            bytes32(uint256(targetData) + 3),
            bytes32(type(uint256).max)
        );
        packing.copy();

        require(uint256(vm.load(address(packing), bytes32(uint256(1)))) == 9, "repack length");
        require(
            uint256(vm.load(address(packing), targetData))
                == ((uint256(1) << 96) | (uint256(2) << 176)),
            "repack slot 0"
        );
        require(
            uint256(vm.load(address(packing), bytes32(uint256(targetData) + 1)))
                == (
                    (uint256(3) << 16) | (uint256(4) << 96) | (uint256(5) << 176)
                ),
            "repack slot 1"
        );
        require(
            uint256(vm.load(address(packing), bytes32(uint256(targetData) + 2)))
                == (
                    (uint256(6) << 16) | (uint256(7) << 96) | (uint256(8) << 176)
                ),
            "repack slot 2"
        );
        require(
            uint256(vm.load(address(packing), bytes32(uint256(targetData) + 3))) == 0,
            "repack cleared tail"
        );

        TupleCleanupLayoutTarget tuple = new TupleCleanupLayoutTarget();
        tuple.clean();
        require(uint256(vm.load(address(tuple), bytes32(uint256(0)))) == 2, "tuple length");
        require(
            uint256(vm.load(address(tuple), keccak256(abi.encode(uint256(0))))) == 0,
            "tuple data"
        );
    }

    function testStorageAliasAndRawCopySlots() public {
        StorageAliasLayoutTarget target = new StorageAliasLayoutTarget();
        bytes32 arrayHead = keccak256(abi.encode(uint256(7), uint256(0)));
        bytes32 arrayData = keccak256(abi.encode(arrayHead));
        vm.store(address(target), arrayHead, bytes32(uint256(1)));
        vm.store(address(target), arrayData, bytes32(type(uint256).max));
        target.selfAssignArray(7);
        require(uint256(vm.load(address(target), arrayHead)) == 1, "array alias length");
        require(
            uint256(vm.load(address(target), arrayData)) == type(uint256).max,
            "array alias data"
        );

        uint256 bytesHead = (uint256(bytes32("abc")) | uint256(6)) | (uint256(0xff) << 8);
        vm.store(address(target), bytes32(uint256(1)), bytes32(bytesHead));
        target.selfAssignBytes();
        require(
            uint256(vm.load(address(target), bytes32(uint256(1)))) == bytesHead,
            "bytes alias"
        );

        bytes32 sourceData = keccak256(abi.encode(uint256(2)));
        bytes32 targetData = keccak256(abi.encode(uint256(3)));
        vm.store(address(target), bytes32(uint256(2)), bytes32(uint256(1)));
        vm.store(address(target), sourceData, bytes32(uint256(2)));
        vm.store(address(target), bytes32(uint256(3)), bytes32(uint256(2)));
        vm.store(address(target), targetData, bytes32(type(uint256).max));
        vm.store(
            address(target),
            bytes32(uint256(targetData) + 1),
            bytes32(type(uint256).max)
        );
        target.copyBools();
        require(uint256(vm.load(address(target), bytes32(uint256(3)))) == 1, "bool copy length");
        require(uint256(vm.load(address(target), targetData)) == 2, "bool raw copy");
        require(
            uint256(vm.load(address(target), bytes32(uint256(targetData) + 1)))
                == type(uint256).max,
            "bool copy unused slot"
        );

        bytes32 kindData = keccak256(abi.encode(uint256(4)));
        vm.store(address(target), bytes32(uint256(4)), bytes32(uint256(1)));
        vm.store(address(target), kindData, bytes32(uint256(2)));
        target.selfAssignKinds();
        require(uint256(vm.load(address(target), bytes32(uint256(4)))) == 1, "enum alias length");
        require(uint256(vm.load(address(target), kindData)) == 2, "enum alias data");
    }

    function testShiftedLayoutBaseSlots() public {
        ShiftedLayoutTarget target = new ShiftedLayoutTarget();
        target.seed();

        require(uint256(vm.load(address(target), bytes32(uint256(0)))) == 0, "unshifted slot");
        require(
            uint256(vm.load(address(target), bytes32(uint256(42))))
                == (
                    uint256(0x112233445566778899aabbccddeeff00)
                        | (uint256(0x0102030405060708) << 128)
                ),
            "shifted packed slot"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(43)))) == 0xabcdef,
            "shifted word"
        );
    }

    function testPackedStateInitializers() public {
        InitializedLayoutTarget target = new InitializedLayoutTarget();
        require(
            uint256(vm.load(address(target), bytes32(uint256(0))))
                == (
                    uint256(0x112233445566778899aabbccddeeff00)
                        | (uint256(0x12345678) << 128)
                ),
            "initialized inheritance"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(1))))
                == (uint256(1) | (uint256(2) << 8) | (uint256(3) << 16)),
            "initialized fixed array"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(2))))
                == 3,
            "initialized dynamic length"
        );
        require(
            uint256(vm.load(address(target), keccak256(abi.encode(uint256(2)))))
                == (uint256(4) | (uint256(5) << 8) | (uint256(6) << 16)),
            "initialized dynamic array"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(3))))
                == (uint256(bytes32("abc")) | uint256(6)),
            "initialized bytes"
        );
        require(
            uint256(vm.load(address(target), bytes32(uint256(4))))
                == (uint256(0x44) | (uint256(0x5566) << 8)),
            "initialized struct"
        );
    }
}
