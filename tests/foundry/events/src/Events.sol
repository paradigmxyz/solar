// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

type Signed16 is int16;

contract Events {
    struct SignedDynamic {
        int8 value;
        bytes data;
    }

    event IndexedSignedDynamic(SignedDynamic indexed value);

    function emitSignedDynamic() external {
        emit IndexedSignedDynamic(SignedDynamic(-1, ""));
    }

    event SignedScalar(int8 indexed value, Signed16 indexed wrapped, int248 data);

    function emitSignedScalar() external {
        emit SignedScalar(-1, Signed16.wrap(-1), -1);
    }

    struct Triple {
        uint256 first;
        uint256 second;
        uint256 third;
    }

    struct DynamicValue {
        uint256 value;
        bytes data;
        uint256[] values;
    }

    enum Choice {
        A,
        B
    }

    struct DynamicScalar {
        uint8 value;
    }

    struct DynamicEnum {
        Choice value;
    }

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event SimpleEvent(uint256 value);
    event IndexedAggregate(Triple[] indexed values);
    event IndexedDynamicAggregate(DynamicValue[] indexed values);
    event IndexedDynamicScalar(DynamicScalar[] indexed values);
    event IndexedDynamicEnum(DynamicEnum[] indexed values);

    function emitSimple(uint256 val) public {
        emit SimpleEvent(val);
    }

    function emitTransfer(address from, address to, uint256 value) public {
        emit Transfer(from, to, value);
    }

    function emitDefaultIndexedAggregate() public {
        Triple[] memory values = new Triple[](1);
        emit IndexedAggregate(values);
    }

    function emitDefaultIndexedDynamicAggregate() public {
        DynamicValue[] memory values = new DynamicValue[](1);
        // A default child must not depend on the contents of scratch memory.
        assembly {
            mstore(0, 7)
        }
        emit IndexedDynamicAggregate(values);
    }

    function emitDirtyIndexedScalar() public {
        DynamicScalar[] memory values = new DynamicScalar[](1);
        assembly {
            mstore(mload(add(values, 0x20)), 0x101)
        }
        emit IndexedDynamicScalar(values);
    }

    function emitInvalidIndexedEnum() public {
        DynamicEnum[] memory values = new DynamicEnum[](1);
        assembly {
            mstore(mload(add(values, 0x20)), 2)
        }
        emit IndexedDynamicEnum(values);
    }

    struct Pair {
        uint256 first;
        uint256 second;
    }

    event TwoWords(uint256 first, uint256 second);

    event TwoWordsIndexed(uint256 indexed id, address account, bool flag);

    event TwoWordsAnonymous(address account, bool flag) anonymous;

    function emitTwoWords(uint256 first, uint256 second) public {
        emit TwoWords(first, second);
    }

    function emitTwoWordsIndexed(uint256 id, address account, bool flag) public {
        emit TwoWordsIndexed(id, account, flag);
    }

    function emitTwoWordsAnonymous(address account, bool flag) public {
        emit TwoWordsAnonymous(account, flag);
    }

    // The fields are read before the data words are written over the memory they alias.
    function emitTwoWordsSwapped() public {
        Pair memory pair;
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            pair := 0
        }
        emit TwoWords(pair.second, pair.first);
    }

    event ThreeWords(uint256 first, uint256 second, uint256 third);

    event DynamicData(uint256 indexed id, bytes data, string text, uint256 count);

    function emitThreeWords(uint256 first, uint256 second, uint256 third) public {
        emit ThreeWords(first, second, third);
    }

    function emitDynamicData(uint256 id, bytes memory data, uint256 count) public {
        emit DynamicData(id, data, "a literal string that is longer than a single word", count);
    }

    event DirtyIndexed(uint8 indexed a, address indexed b, bool indexed c, bytes4 d);
    event DirtyData(uint8 a, address b, bool c, bytes4 d, int8 e);
    event DirtyIndexedChoice(Choice indexed choice);

    // The dirty words and the expected logs below come from `solc --via-ir`.
    function emitDirtyIndexed() external {
        uint8 a;
        address b;
        bool c;
        bytes4 d;
        assembly {
            a := 0xabcd1b
            b := or(0x1234567890123456789012345678901234567890, shl(160, 0xabcd))
            c := 0x100
            d := or(shl(224, 0xdeadbeef), 0xabcdef)
        }
        emit DirtyIndexed(a, b, c, d);
    }

    function emitDirtyData() external {
        uint8 a;
        address b;
        bool c;
        bytes4 d;
        int8 e;
        assembly {
            a := 0xabcd1b
            b := or(0x1234567890123456789012345678901234567890, shl(160, 0xabcd))
            c := 0x100
            d := or(shl(224, 0xdeadbeef), 0xabcdef)
            e := 0xabcdfd
        }
        emit DirtyData(a, b, c, d, e);
    }

    function emitDirtyIndexedChoice() external {
        Choice choice;
        assembly {
            choice := 0x101
        }
        emit DirtyIndexedChoice(choice);
    }

    struct Late {
        uint256 value;
    }

    event LateStorage(uint256[] values, Late late, uint256 count);
    event LateIndexed(uint256[] indexed values, Late indexed late, uint256 indexed count);
    event LateIndexedText(string indexed text, uint256 indexed count);

    uint256[] internal lateValues;
    Late internal late;
    string internal lateText = "before";

    // Indexed storage topics are hashed after the indexed arguments that follow them ran.
    function emitLateIndexed() public {
        emit LateIndexed(lateValues, late, writeLate());
    }

    function emitLateIndexedText() public {
        emit LateIndexedText(lateText, writeLate());
    }

    // The data reads the storage array and struct after the last argument ran, as solc does.
    function emitLateStorage() public {
        emit LateStorage(lateValues, late, writeLate());
    }

    function writeLate() internal returns (uint256) {
        lateValues.push(7);
        late.value = 9;
        lateText = "after";
        return 3;
    }

    event StagedBytes(bytes indexed value);
    event StagedText(string indexed value);
    event StagedPair(Pair indexed value);

    // Indexed hash inputs are staged at the free memory pointer, never reserved.
    function emitStagedBytes(bytes calldata value) public {
        emit StagedBytes(value);
    }

    function emitStagedBytesSlice(bytes calldata value) public {
        emit StagedBytes(value[1:3]);
    }

    function emitStagedMemoryBytes(bytes memory value) public {
        emit StagedBytes(value);
    }

    function emitStagedText(string calldata value) public {
        emit StagedText(value);
    }

    function emitStagedPair(Pair calldata value) public {
        emit StagedPair(value);
    }

    function emitStagedMemoryPair(uint256 first, uint256 second) public {
        Pair memory value = Pair(first, second);
        emit StagedPair(value);
    }

    event StagedMixed(Pair indexed pair, bytes indexed one, bytes data, bytes indexed two, uint256 count);

    function emitStagedMixed(Pair calldata pair, bytes calldata one, bytes calldata data, bytes calldata two)
        public
    {
        emit StagedMixed(pair, one, data, two, 7);
    }
}
