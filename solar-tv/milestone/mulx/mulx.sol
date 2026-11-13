// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Main {
    uint8 x8;
    uint16 y16 = 3;
    uint256 z;
    function m(uint8 a, uint16 b) external { x8 *= a; y16 *= b; }
    function n(uint8 a, uint16 b, uint256 c) external returns (uint256) { z = uint256(a) * b * c; return z; }
}
