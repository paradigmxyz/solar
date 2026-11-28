// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    mapping(bytes4 => uint256) m;
    // expect: has a `bytesN` mapping key
    /// @custom:ensures m[k] == 0
    function f(bytes4 k) external {}
}
