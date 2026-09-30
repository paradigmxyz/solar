// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Strings} from "solar:core/Strings.sol";

contract UsesModules {
    function text(uint256 value) external pure returns (string memory) {
        return Strings.toString(value);
    }
}

contract Plain {
    function one() external pure returns (uint256) {
        return 1;
    }
}
