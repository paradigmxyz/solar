// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

contract Packed {
    struct Entry {
        uint128 a;
        uint128 b;
        bool c;
    }

    mapping(address => Entry) entries;

    function set(address who, uint128 a, uint128 b, bool c) external {
        Entry storage e = entries[who];
        e.a = a;
        e.b = b;
        e.c = c;
    }

    function setB(address who, uint128 b) external {
        entries[who].b = b;
    }

    function get(address who) external view returns (uint128, uint128, bool) {
        Entry storage e = entries[who];
        return (e.a, e.b, e.c);
    }
}
