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

    // Static event data lives in scratch memory: the heap and the free memory pointer stay put.
    function emitTwoWordsKeepsHeap(uint256 first, uint256 second) public returns (bool) {
        bytes memory buffer = new bytes(64);
        assembly {
            mstore(add(buffer, 0x20), 0x1111)
            mstore(add(buffer, 0x40), 0x2222)
        }
        uint256 pointer;
        assembly {
            pointer := mload(0x40)
        }
        emit TwoWords(first, second);
        uint256 afterPointer;
        uint256 one;
        uint256 two;
        assembly {
            afterPointer := mload(0x40)
            one := mload(add(buffer, 0x20))
            two := mload(add(buffer, 0x40))
        }
        return afterPointer == pointer && one == 0x1111 && two == 0x2222;
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
}
