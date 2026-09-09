// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {LibString} from "src/utils/LibString.sol";

// Isolate normal ABI decoding, library execution, and return encoding from the
// upstream test contract's assertions and deliberate memory brutalization.
contract Encoding {
    function hexNoPrefix(bytes memory input) external pure returns (string memory) {
        return LibString.toHexStringNoPrefix(input);
    }

    function hexPrefixed(bytes memory input) external pure returns (string memory) {
        return LibString.toHexString(input);
    }

    function ascii(bytes memory input) external pure returns (bool) {
        return LibString.is7BitASCII(string(input));
    }
}
