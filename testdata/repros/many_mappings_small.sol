// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ManyMappings {
    mapping(address => uint256) public map0;
    mapping(uint256 => mapping(address => uint256)) public nestedMap1;
    mapping(bytes32 => address) public hashMap2;
    mapping(address => mapping(uint256 => bool)) public doubleMap3;
    mapping(address => uint256) public map4;
    mapping(uint256 => mapping(address => uint256)) public nestedMap5;
    mapping(bytes32 => address) public hashMap6;
    mapping(address => mapping(uint256 => bool)) public doubleMap7;
    mapping(address => uint256) public map8;
    mapping(uint256 => mapping(address => uint256)) public nestedMap9;

    function setAll(address addr, uint256 val) public {
        map0[addr] = val;
        nestedMap1[val][addr] = val;
        hashMap2[keccak256(abi.encode(val))] = addr;
        doubleMap3[addr][val] = true;
        map4[addr] = val;
        nestedMap5[val][addr] = val;
        hashMap6[keccak256(abi.encode(val))] = addr;
        doubleMap7[addr][val] = true;
        map8[addr] = val;
        nestedMap9[val][addr] = val;
    }
}
