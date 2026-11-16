// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// `delete` of mapping entries at variable keys, of each mapping shape, with keys that may alias
// (a == b) and entries cleared in one function and read in another.
contract MappingDelete {
    mapping(address => uint256) public balance;
    mapping(uint256 => bool) public seen;
    mapping(address => mapping(address => uint256)) public allowed;

    function set(address who, uint256 v) external { balance[who] = v; }
    function clear(address who) external { delete balance[who]; }
    function get(address who) external view returns (uint256) { return balance[who]; }

    // After writing a and deleting b, the read of a is 0 exactly when a == b.
    function aliasWrite(address a, address b, uint256 v) external returns (uint256) {
        balance[a] = v;
        delete balance[b];
        return balance[a];
    }

    function unmark(uint256 id) external { delete seen[id]; }
    function revoke(address owner, address spender) external { delete allowed[owner][spender]; }
    function revokeRead(address owner, address spender) external returns (uint256) {
        allowed[owner][spender] = 7;
        delete allowed[owner][spender];
        return allowed[owner][spender];
    }
}
