// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

struct DynamicInputItem {
    uint256 value;
    address owner;
}

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

    function probeNested(uint256[][] calldata values) external pure returns (uint256) {
        if (
            values.length == 2
                && values[0].length == 1
                && values[1].length == 2
                && values[0][0] == 7
                && values[1][0] == 42
                && values[1][1] == 99
        ) {
            return 2;
        }
        return values.length;
    }

    function probeStructArray(
        DynamicInputItem[] calldata values
    ) external pure returns (uint256) {
        if (
            values.length == 1
                && values[0].value == 42
                && values[0].owner == address(0x1234)
        ) {
            return 2;
        }
        return values.length;
    }
}
