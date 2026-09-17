// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library OverloadLib {
    function value() external pure returns (uint256) {
        return 99;
    }
}
