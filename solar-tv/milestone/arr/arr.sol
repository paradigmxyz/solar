// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Arr {
    uint256[] xs;
    function push(uint256 v) external { xs.push(v); }
    function length() external view returns (uint256) { return xs.length; }
    function at(uint256 i) external view returns (uint256) { return xs[i]; }
}
