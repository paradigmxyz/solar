// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.13;

contract MinimalProxy {
    address private immutable implementation = address(new MinimalProxyImplementation());

    fallback(bytes calldata input) external payable returns (bytes memory) {
        (bool success, bytes memory output) = implementation.delegatecall(input);
        if (!success) {
            assembly ("memory-safe") {
                revert(add(output, 32), mload(output))
            }
        }
        return output;
    }
}

contract MinimalProxyImplementation {
    uint256 public number;

    function setNumber(uint256 value) external {
        number = value;
    }

    function echo(bytes calldata data) external pure returns (bytes memory) {
        return data;
    }
}
