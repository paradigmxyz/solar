// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Mapping - Contract with mapping storage for equivalence testing
contract Mapping {
    mapping(address => uint256) public balances;

    function set(address a, uint256 v) external {
        balances[a] = v;
    }

    function get(address a) external view returns (uint256) {
        return balances[a];
    }
}
