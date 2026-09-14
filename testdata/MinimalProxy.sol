// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.13;

contract MinimalProxy {
    address private immutable implementation = address(new MinimalProxyImplementation());

    fallback() external payable {
        address target = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let success := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            if iszero(success) { revert(0, returndatasize()) }
            return(0, returndatasize())
        }
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
