// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Child {}

contract Twin {}

contract Factory {
    function make() external returns (address) {
        return address(new Child());
    }

    function make2(bytes32 salt) external returns (address) {
        return address(new Twin{salt: salt}());
    }
}
