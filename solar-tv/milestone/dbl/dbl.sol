// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Dbl {
    function double(uint256 x) external pure returns (uint256) {
        return x * 2;
    }
}
