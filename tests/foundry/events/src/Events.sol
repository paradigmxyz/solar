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

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event SimpleEvent(uint256 value);
    event IndexedAggregate(Triple[] indexed values);
    event IndexedDynamicAggregate(DynamicValue[] indexed values);

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
}
