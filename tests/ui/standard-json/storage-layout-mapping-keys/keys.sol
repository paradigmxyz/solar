// SPDX-License-Identifier: MIT
pragma solidity ^0.9.0;

contract MappingKeys {
    struct S {
        mapping(string => uint256) m;
    }

    mapping(string => address) byString;
    mapping(bytes => uint256) byBytes;
    mapping(string => mapping(bytes => uint256)) nested;
    S s;
    mapping(string => uint256)[] arr;
    mapping(bytes32 => uint256) byValue;
}
