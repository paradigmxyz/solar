// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library GuardLib {
    function pair(uint256 value) public pure returns (uint256, bytes memory) {
        return (value, abi.encode(value + 1));
    }

    function bump(uint256[] storage a, uint256 v) external returns (uint256) {
        a.push(v);
        return a.length;
    }

    function peek(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }

    function twice(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }
}

contract GuardUser {
    using GuardLib for uint256;

    function pair(uint256 value, bool attached) external pure returns (uint256 first, uint256 second) {
        bytes memory data;
        if (attached) {
            (first, data) = value.pair();
        } else {
            (first, data) = GuardLib.pair(value);
        }
        second = abi.decode(data, (uint256));
        assembly { mstore(0x40, 0x80) }
    }

    uint256[] internal values;

    // The library sees this call's value through `DELEGATECALL` and must accept it.
    function bump(uint256 v) external payable returns (uint256) {
        return GuardLib.bump(values, v);
    }

    function peek() external view returns (uint256) {
        return GuardLib.peek(values);
    }
}
