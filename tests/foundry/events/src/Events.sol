// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Events {
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
}
