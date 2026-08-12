// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ImportedHelper} from "@helpers/ImportedHelper.sol";

contract RemappedDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        return ImportedHelper.normalize(value);
    }
}
