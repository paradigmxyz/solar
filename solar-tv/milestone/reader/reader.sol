// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IOracle {
    function price() external view returns (uint256);
}

contract Reader {
    function read(IOracle o) external view returns (uint256) {
        return o.price() + 1;
    }
}

