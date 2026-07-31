//@ run-call: PackedSolcLayout::scalarLayout => 0, 0, 0, 3, 0, 4, 0, 28
//@ run-call: PackedSolcLayout::aggregateLayout => 1, 4, 5, 6, 7, 8
//@ run-call: PackedSolcLayout::scalarPacking => true
//@ run-call: PackedSolcLayout::aggregatePacking => true
//@ run-call: PackedSolcLayout::dynamicArrayPacking => 1, 254, 3, 2
//@ run-call: PackedSolcLayout::mappingPacking => 0xabcdef, -7, 0xabcdef, 249
//@ run-call: PackedSolcLayout::mappingAssignmentPreservesPadding => true
//@ run-call: PackedSolcLayout::memoryDynamicCopyPacking => true
//@ run-call: PackedSolcLayout::memoryDynamicCopyShrinkClears => true
//@ run-call: PackedSolcLayout::dynamicArrayDeleteClears => true
//@ run-call: PackedSolcLayout::nestedDynamicArrayDeleteClears => true
//@ run-call: PackedSolcLayout::nestedDynamicArrayElementDeleteClears => true
//@ run-call: PackedSolcLayout::mappingArrayDeleteOnlyClearsLength => true
//@ run-call: PackedSolcLayout::structDeletePreservesMappingSlot => true
//@ run-call: PackedSolcLayout::memoryStructCopyPacking => true
//@ run-call: PackedSolcLayout::dynamicStructDeleteClears => true
//@ run-call: PackedSolcLayout::memoryFixedCopyPacking => true
//@ run-call: NestedPackedLayout::layout => 0, 5
//@ run-call: NestedPackedLayout::packing => true
//@ run-call: PackedArrayWidths::layout => 0, 2, 3
//@ run-call: PackedArrayWidths::fixedPacking => true
//@ run-call: PackedArrayWidths::dynamicBytesPacking => true
//@ run-call: PackedArrayWidths::memoryDynamicCopyClearsPadding => true
//@ run-call: PackedArrayWidths::signedPacking => -1, -8388608, 8388607, 0x7fffff800000ffffff
//@ run-call: PackedDerived::layout => 0, 0, 1, 0, 1, 16, 2, 0
//@ run-call: PackedDerived::packing => true
//@ run-call: PackedScalarKinds::packing => true
//@ run-call: LargeStorageLayout::layout => 0x18000000000000000

contract PackedSolcLayout {
    bytes3 private smallBytes;
    uint8 private afterBytes;
    function() external returns (uint256) private callback;
    uint8 private afterCallback;

    struct Small {
        uint8 a;
        uint8 b;
        uint256 wide;
        uint8 c;
    }

    Small private small;
    uint8[4] private fixedItems;
    uint8 private afterItems;
    uint8[] private dynamicItems;
    mapping(uint256 => bytes3) private mappedBytes;
    mapping(uint256 => int8) private mappedSigned;

    struct DeepPacked {
        uint8 a;
        uint8 b;
        uint8[] items;
        uint8 c;
    }

    uint8[] private copiedDynamic;
    DeepPacked private copiedStruct;
    uint8[4] private copiedFixed;
    uint8[][] private nestedItems;
    mapping(uint256 => uint256)[] private mappingItems;

    struct WithMapping {
        uint8 beforeMapping;
        mapping(uint256 => uint256) items;
        uint8 afterMapping;
    }

    WithMapping private withMapping;

    function target() external pure returns (uint256) {
        return 77;
    }

    function scalarLayout()
        external
        pure
        returns (
            uint256 bytesSlot,
            uint256 bytesOffset,
            uint256 afterBytesSlot,
            uint256 afterBytesOffset,
            uint256 callbackSlot,
            uint256 callbackOffset,
            uint256 afterCallbackSlot,
            uint256 afterCallbackOffset
        )
    {
        assembly {
            bytesSlot := smallBytes.slot
            bytesOffset := smallBytes.offset
            afterBytesSlot := afterBytes.slot
            afterBytesOffset := afterBytes.offset
            callbackSlot := callback.slot
            callbackOffset := callback.offset
            afterCallbackSlot := afterCallback.slot
            afterCallbackOffset := afterCallback.offset
        }
    }

    function aggregateLayout()
        external
        pure
        returns (
            uint256 smallSlot,
            uint256 fixedSlot,
            uint256 afterSlot,
            uint256 dynamicSlot,
            uint256 bytesMapSlot,
            uint256 signedMapSlot
        )
    {
        assembly {
            smallSlot := small.slot
            fixedSlot := fixedItems.slot
            afterSlot := afterItems.slot
            dynamicSlot := dynamicItems.slot
            bytesMapSlot := mappedBytes.slot
            signedMapSlot := mappedSigned.slot
        }
    }

    function scalarPacking() external returns (bool) {
        smallBytes = hex"abcdef";
        afterBytes = 0xff;
        callback = this.target;
        afterCallback = 0x5a;

        uint256 raw;
        assembly {
            raw := sload(smallBytes.slot)
        }
        return smallBytes == hex"abcdef" && afterBytes == 0xff
            && callback.address == address(this) && callback.selector == this.target.selector
            && afterCallback == 0x5a && uint24(raw) == 0xabcdef
            && uint8(raw >> 24) == 0xff && uint32(raw >> 32) == uint32(this.target.selector)
            && uint160(raw >> 64) == uint160(address(this)) && uint8(raw >> 224) == 0x5a
            && raw >> 232 == 0;
    }

    function aggregatePacking() external returns (bool) {
        small = Small(1, 2, 99, 3);
        fixedItems[0] = 1;
        fixedItems[1] = 2;
        fixedItems[2] = 3;
        fixedItems[3] = 4;
        afterItems = 9;

        uint256 smallFirst;
        uint256 smallWide;
        uint256 smallLast;
        uint256 fixedWord;
        uint256 afterWord;
        assembly {
            smallFirst := sload(small.slot)
            smallWide := sload(add(small.slot, 1))
            smallLast := sload(add(small.slot, 2))
            fixedWord := sload(fixedItems.slot)
            afterWord := sload(afterItems.slot)
        }
        return small.a == 1 && small.b == 2 && small.wide == 99 && small.c == 3
            && fixedItems[0] == 1 && fixedItems[1] == 2 && fixedItems[2] == 3
            && fixedItems[3] == 4 && afterItems == 9 && smallFirst == 0x0201
            && smallWide == 99 && smallLast == 3 && fixedWord == 0x04030201 && afterWord == 9;
    }

    function dynamicArrayPacking() external returns (uint8, uint8, uint8, uint256) {
        dynamicItems.push(1);
        dynamicItems.push(2);
        dynamicItems.push(3);
        dynamicItems[1] = 254;
        uint8 removed = dynamicItems[2];
        dynamicItems.pop();
        return (dynamicItems[0], dynamicItems[1], removed, dynamicItems.length);
    }

    function mappingPacking() external returns (bytes3, int8, uint256, uint256) {
        mappedBytes[42] = hex"abcdef";
        mappedSigned[42] = -7;

        uint256 bytesWord;
        uint256 signedWord;
        assembly {
            mstore(0, 42)
            mstore(32, mappedBytes.slot)
            bytesWord := sload(keccak256(0, 64))
            mstore(32, mappedSigned.slot)
            signedWord := sload(keccak256(0, 64))
        }
        return (mappedBytes[42], mappedSigned[42], bytesWord, signedWord);
    }

    function mappingAssignmentPreservesPadding() external returns (bool) {
        uint256 bytesSlot;
        uint256 signedSlot;
        assembly {
            mstore(0, 7)
            mstore(32, mappedBytes.slot)
            bytesSlot := keccak256(0, 64)
            sstore(bytesSlot, not(0))
            mstore(32, mappedSigned.slot)
            signedSlot := keccak256(0, 64)
            sstore(signedSlot, not(0))
        }
        mappedBytes[7] = hex"abcdef";
        mappedSigned[7] = -7;

        uint256 bytesWord;
        uint256 signedWord;
        assembly {
            bytesWord := sload(bytesSlot)
            signedWord := sload(signedSlot)
        }
        return bytesWord == ((type(uint256).max << 24) | uint256(0xabcdef))
            && signedWord == ((type(uint256).max << 8) | uint256(0xf9));
    }

    function memoryDynamicCopyPacking() external returns (bool) {
        uint8[] memory items = new uint8[](35);
        items[0] = 1;
        items[31] = 32;
        items[32] = 33;
        items[34] = 35;
        assembly {
            mstore(0, copiedDynamic.slot)
            let data := keccak256(0, 32)
            sstore(data, not(0))
            sstore(add(data, 1), not(0))
        }
        copiedDynamic = items;

        uint256 dynamicFirst;
        uint256 dynamicSecond;
        assembly {
            mstore(0, copiedDynamic.slot)
            let dynamicData := keccak256(0, 32)
            dynamicFirst := sload(dynamicData)
            dynamicSecond := sload(add(dynamicData, 1))
        }
        return copiedDynamic.length == 35 && copiedDynamic[0] == 1
            && copiedDynamic[31] == 32 && copiedDynamic[32] == 33
            && copiedDynamic[34] == 35
            && dynamicFirst == ((uint256(32) << 248) | 1) && dynamicSecond == 0x230021;
    }

    function memoryDynamicCopyShrinkClears() external returns (bool) {
        uint8[] memory large = new uint8[](35);
        large[0] = 1;
        large[1] = 2;
        large[31] = 32;
        large[32] = 33;
        large[34] = 35;
        copiedDynamic = large;

        uint8[] memory smallItems = new uint8[](1);
        smallItems[0] = 9;
        copiedDynamic = smallItems;

        uint256 firstWord;
        uint256 secondWord;
        assembly {
            mstore(0, copiedDynamic.slot)
            let data := keccak256(0, 32)
            firstWord := sload(data)
            secondWord := sload(add(data, 1))
        }
        return copiedDynamic.length == 1 && copiedDynamic[0] == 9
            && firstWord == 9 && secondWord == 0;
    }

    function dynamicArrayDeleteClears() external returns (bool) {
        dynamicItems.push(1);
        dynamicItems.push(2);
        dynamicItems.push(3);
        delete dynamicItems;

        uint256 dataWord;
        assembly {
            mstore(0, dynamicItems.slot)
            dataWord := sload(keccak256(0, 32))
        }
        return dynamicItems.length == 0 && dataWord == 0;
    }

    function nestedDynamicArrayDeleteClears() external returns (bool) {
        nestedItems.push();
        nestedItems[0].push(7);
        delete nestedItems;

        uint256 innerLength;
        uint256 innerDataWord;
        assembly {
            mstore(0, nestedItems.slot)
            let outerData := keccak256(0, 32)
            innerLength := sload(outerData)
            mstore(0, outerData)
            innerDataWord := sload(keccak256(0, 32))
        }
        return nestedItems.length == 0 && innerLength == 0 && innerDataWord == 0;
    }

    function nestedDynamicArrayElementDeleteClears() external returns (bool) {
        nestedItems.push();
        nestedItems[0].push(7);
        delete nestedItems[0];

        uint256 innerLength;
        uint256 innerDataWord;
        assembly {
            mstore(0, nestedItems.slot)
            let outerData := keccak256(0, 32)
            innerLength := sload(outerData)
            mstore(0, outerData)
            innerDataWord := sload(keccak256(0, 32))
        }
        return nestedItems.length == 1 && nestedItems[0].length == 0
            && innerLength == 0 && innerDataWord == 0;
    }

    function mappingArrayDeleteOnlyClearsLength() external returns (bool) {
        assembly {
            sstore(mappingItems.slot, not(0))
        }
        delete mappingItems;
        return mappingItems.length == 0;
    }

    function structDeletePreservesMappingSlot() external returns (bool) {
        assembly {
            sstore(withMapping.slot, not(0))
            sstore(add(withMapping.slot, 2), not(0))
        }
        withMapping.beforeMapping = 1;
        withMapping.afterMapping = 2;
        assembly {
            sstore(add(withMapping.slot, 1), 0xabcdef)
        }
        delete withMapping;

        uint256 mappingSlot;
        uint256 head;
        uint256 tail;
        assembly {
            head := sload(withMapping.slot)
            mappingSlot := sload(add(withMapping.slot, 1))
            tail := sload(add(withMapping.slot, 2))
        }
        return withMapping.beforeMapping == 0 && withMapping.afterMapping == 0
            && head == (type(uint256).max << 8) && mappingSlot == 0xabcdef
            && tail == (type(uint256).max << 8);
    }

    function memoryStructCopyPacking() external returns (bool) {
        uint8[] memory items = new uint8[](35);
        items[0] = 1;
        items[31] = 32;
        items[32] = 33;
        items[34] = 35;
        copiedStruct = DeepPacked(7, 8, items, 9);

        uint256 structHead;
        uint256 structFirst;
        uint256 structSecond;
        uint256 structTail;
        assembly {
            structHead := sload(copiedStruct.slot)
            mstore(0, add(copiedStruct.slot, 1))
            let structData := keccak256(0, 32)
            structFirst := sload(structData)
            structSecond := sload(add(structData, 1))
            structTail := sload(add(copiedStruct.slot, 2))
        }
        return copiedStruct.a == 7 && copiedStruct.b == 8
            && copiedStruct.items.length == 35 && copiedStruct.items[0] == 1
            && copiedStruct.items[31] == 32 && copiedStruct.items[32] == 33
            && copiedStruct.items[34] == 35 && copiedStruct.c == 9 && structHead == 0x0807
            && structFirst == ((uint256(32) << 248) | 1)
            && structSecond == 0x230021 && structTail == 9;
    }

    function dynamicStructDeleteClears() external returns (bool) {
        uint8[] memory items = new uint8[](35);
        items[0] = 1;
        items[34] = 35;
        copiedStruct = DeepPacked(7, 8, items, 9);
        delete copiedStruct;

        uint256 head;
        uint256 firstData;
        uint256 secondData;
        uint256 tail;
        assembly {
            head := sload(copiedStruct.slot)
            mstore(0, add(copiedStruct.slot, 1))
            let data := keccak256(0, 32)
            firstData := sload(data)
            secondData := sload(add(data, 1))
            tail := sload(add(copiedStruct.slot, 2))
        }
        return head == 0 && firstData == 0 && secondData == 0 && tail == 0;
    }

    function memoryFixedCopyPacking() external returns (bool) {
        assembly {
            sstore(copiedFixed.slot, not(0))
        }
        copiedFixed = [uint8(1), 2, 3, 4];

        uint256 fixedWord;
        assembly {
            fixedWord := sload(copiedFixed.slot)
        }
        return copiedFixed[0] == 1 && copiedFixed[1] == 2
            && copiedFixed[2] == 3 && copiedFixed[3] == 4 && fixedWord == 0x04030201;
    }
}

contract NestedPackedLayout {
    struct Inner {
        uint8 a;
        uint8 b;
        uint256 wide;
        uint8 c;
    }

    struct Outer {
        uint8 lead;
        Inner inner;
        uint8 tail;
    }

    Outer private value;
    uint8 private afterValue;

    function layout() external pure returns (uint256 valueSlot, uint256 afterSlot) {
        assembly {
            valueSlot := value.slot
            afterSlot := afterValue.slot
        }
    }

    function packing() external returns (bool) {
        value = Outer(7, Inner(8, 9, 10, 11), 12);
        afterValue = 13;

        uint256 outerLead;
        uint256 innerFirst;
        uint256 innerWide;
        uint256 innerLast;
        uint256 outerTail;
        uint256 afterWord;
        assembly {
            outerLead := sload(value.slot)
            innerFirst := sload(add(value.slot, 1))
            innerWide := sload(add(value.slot, 2))
            innerLast := sload(add(value.slot, 3))
            outerTail := sload(add(value.slot, 4))
            afterWord := sload(afterValue.slot)
        }
        return value.lead == 7 && value.inner.a == 8 && value.inner.b == 9
            && value.inner.wide == 10 && value.inner.c == 11 && value.tail == 12
            && afterValue == 13 && outerLead == 7 && innerFirst == 0x0908
            && innerWide == 10 && innerLast == 11 && outerTail == 12 && afterWord == 13;
    }
}

contract PackedArrayWidths {
    bytes3[12] private fixedBytes;
    bytes3[] private dynamicBytes;
    int24[] private signedItems;

    function layout()
        external
        pure
        returns (uint256 fixedSlot, uint256 dynamicSlot, uint256 signedSlot)
    {
        assembly {
            fixedSlot := fixedBytes.slot
            dynamicSlot := dynamicBytes.slot
            signedSlot := signedItems.slot
        }
    }

    function fixedPacking() external returns (bool) {
        fixedBytes[9] = hex"abcdef";
        fixedBytes[10] = hex"123456";
        fixedBytes[11] = hex"789abc";

        uint256 firstWord;
        uint256 secondWord;
        assembly {
            firstWord := sload(fixedBytes.slot)
            secondWord := sload(add(fixedBytes.slot, 1))
        }
        return fixedBytes[9] == hex"abcdef" && fixedBytes[10] == hex"123456"
            && fixedBytes[11] == hex"789abc" && uint24(firstWord >> 216) == 0xabcdef
            && secondWord == 0x789abc123456;
    }

    function dynamicBytesPacking() external returns (bool) {
        for (uint256 i; i < 10; ++i) {
            dynamicBytes.push(bytes3(uint24(i + 1)));
        }
        dynamicBytes.push(hex"abcdef");

        uint256 firstWord;
        uint256 secondWord;
        uint256 slot;
        assembly {
            mstore(0, dynamicBytes.slot)
            slot := keccak256(0, 32)
            firstWord := sload(slot)
            secondWord := sload(add(slot, 1))
        }
        return dynamicBytes[9] == bytes3(uint24(10)) && dynamicBytes[10] == hex"abcdef"
            && uint24(firstWord >> 216) == 10 && secondWord == 0xabcdef;
    }

    function memoryDynamicCopyClearsPadding() external returns (bool) {
        bytes3[] memory items = new bytes3[](11);
        items[0] = hex"010203";
        items[9] = hex"abcdef";
        items[10] = hex"123456";
        assembly {
            mstore(0, dynamicBytes.slot)
            let data := keccak256(0, 32)
            sstore(data, not(0))
            sstore(add(data, 1), not(0))
        }
        dynamicBytes = items;

        uint256 firstWord;
        uint256 secondWord;
        assembly {
            mstore(0, dynamicBytes.slot)
            let data := keccak256(0, 32)
            firstWord := sload(data)
            secondWord := sload(add(data, 1))
        }
        return dynamicBytes.length == 11 && dynamicBytes[0] == hex"010203"
            && dynamicBytes[9] == hex"abcdef" && dynamicBytes[10] == hex"123456"
            && firstWord == ((uint256(0xabcdef) << 216) | uint256(0x010203))
            && secondWord == 0x123456;
    }

    function signedPacking() external returns (int24, int24, int24, uint256) {
        signedItems.push(-1);
        signedItems.push(-8388608);
        signedItems.push(8388607);

        uint256 word;
        assembly {
            mstore(0, signedItems.slot)
            word := sload(keccak256(0, 32))
        }
        return (signedItems[0], signedItems[1], signedItems[2], word);
    }
}

contract PackedScalarKinds {
    enum Kind {
        Zero,
        One,
        Two
    }

    Kind private kind;
    PackedScalarKinds private peer;
    function(uint256) internal pure returns (uint256) private operation;
    uint8 private sentinel;

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function packing() external returns (bool) {
        kind = Kind.Two;
        peer = this;
        operation = increment;
        sentinel = 0x5a;

        uint256 raw;
        assembly {
            raw := sload(kind.slot)
        }
        uint256 internalMask = type(uint64).max;
        return kind == Kind.Two && address(peer) == address(this)
            && operation(41) == 42 && sentinel == 0x5a && uint8(raw) == 2
            && address(uint160(raw >> 8)) == address(this)
            && ((raw >> 168) & internalMask) != 0 && uint8(raw >> 232) == 0x5a
            && raw >> 240 == 0;
    }
}

contract PackedBase {
    uint256 internal inheritedWord;
    uint128 internal inheritedSmall;
}

contract PackedDerived is PackedBase {
    uint32 private derivedSmall;
    uint256 private derivedWord;

    function layout()
        external
        pure
        returns (
            uint256 inheritedWordSlot,
            uint256 inheritedWordOffset,
            uint256 inheritedSmallSlot,
            uint256 inheritedSmallOffset,
            uint256 derivedSmallSlot,
            uint256 derivedSmallOffset,
            uint256 derivedWordSlot,
            uint256 derivedWordOffset
        )
    {
        assembly {
            inheritedWordSlot := inheritedWord.slot
            inheritedWordOffset := inheritedWord.offset
            inheritedSmallSlot := inheritedSmall.slot
            inheritedSmallOffset := inheritedSmall.offset
            derivedSmallSlot := derivedSmall.slot
            derivedSmallOffset := derivedSmall.offset
            derivedWordSlot := derivedWord.slot
            derivedWordOffset := derivedWord.offset
        }
    }

    function packing() external returns (bool) {
        inheritedSmall = 0x112233445566778899aabbccddeeff00;
        derivedSmall = 0x12345678;

        uint256 packed;
        assembly {
            packed := sload(inheritedSmall.slot)
        }
        return inheritedSmall == 0x112233445566778899aabbccddeeff00
            && derivedSmall == 0x12345678
            && uint128(packed) == 0x112233445566778899aabbccddeeff00
            && uint32(packed >> 128) == 0x12345678 && packed >> 160 == 0;
    }
}

contract LargeStorageLayout {
    uint256[9223372036854775808][3] private huge;
    uint256 private afterHuge;

    function layout() external pure returns (uint256 slot) {
        assembly {
            slot := afterHuge.slot
        }
    }
}
