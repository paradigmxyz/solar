// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test-only oracle mutant. Each difference requires both the configured
// dynamic shape and symbolic element values, and must survive both replays.
contract DynamicInputDifferential {
    function probeBytes(bytes calldata data) external pure returns (uint256) {
        if (
            data.length == 3
                && data[0] == 0x61
                && data[1] == 0x62
                && data[2] == 0x63
        ) {
            return 2;
        }
        return data.length;
    }

    function probeArray(uint256[] calldata values) external pure returns (uint256) {
        if (
            values.length == 2
                && values[0] == 42
                && values[1] == 99
        ) {
            return 2;
        }
        return values.length;
    }
}
