//@ run-call: PackedSolcLayout::scalarLayout => 0, 0, 0, 3, 0, 4, 0, 28
//@ run-call: PackedSolcLayout::aggregateLayout => 1, 4, 5, 6, 7, 8
//@ run-call: PackedSolcLayout::scalarPacking => true
//@ run-call: PackedSolcLayout::aggregatePacking => true
//@ run-call: PackedSolcLayout::dynamicArrayPacking => 1, 254, 3, 2
//@ run-call: PackedSolcLayout::mappingPacking => 0xabcdef, -7, 0xabcdef, 249
//@ run-call: NestedPackedLayout::layout => 0, 5
//@ run-call: NestedPackedLayout::packing => true
//@ run-call: PackedArrayWidths::layout => 0, 2, 3
//@ run-call: PackedArrayWidths::fixedPacking => true
//@ run-call: PackedArrayWidths::dynamicBytesPacking => true
//@ run-call: PackedArrayWidths::signedPacking => -1, -8388608, 8388607, 0x7fffff800000ffffff
//@ run-call: PackedDerived::layout => 0, 0, 1, 0, 1, 16, 2, 0
//@ run-call: PackedDerived::packing => true

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
