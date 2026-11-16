// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Generated getters of public mappings: a word mapping keyed by address, a bool mapping keyed by
// uint256 and a nested mapping, each written by one function and read back through its getter.
contract MappingGetter {
    mapping(address => uint256) public balance;
    mapping(uint256 => bool) public seen;
    mapping(address => mapping(address => uint256)) public allowed;

    function setBalance(address who, uint256 v) external { balance[who] = v; }
    function mark(uint256 id) external { seen[id] = true; }
    function approve(address owner, address spender, uint256 v) external { allowed[owner][spender] = v; }
}
