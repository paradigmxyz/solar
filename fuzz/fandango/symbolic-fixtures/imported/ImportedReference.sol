// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ImportedHelper} from "./ImportedHelper.sol";

contract ImportedDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        return ImportedHelper.normalize(value);
    }
}
