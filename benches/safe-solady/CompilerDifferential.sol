// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {SafeCastLib} from "./src/utils/SafeCastLib.sol";
import {LibBit} from "./src/utils/LibBit.sol";

/// @notice Small targets for the repository's solsymdiff compiler check.
contract CompilerDifferential {
    function narrow(int256 x) external pure returns (int128) {
        return SafeCastLib.toInt128(x);
    }

    function popCount(uint256 x) external pure returns (uint256) {
        return LibBit.popCount(x);
    }
}
