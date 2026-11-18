// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Reverter {
    error Failed(uint256 code);

    function fail() external pure {
        revert Failed(7);
    }

    function echo(uint256 value) external pure returns (uint256) {
        return value;
    }
}

/// Assigns low-level call results straight to storage in a tuple assignment.
contract StoredCallResults {
    bool public ok;
    bytes public result;

    function call(address target, bytes calldata data) external {
        (ok, result) = target.call(data);
    }

    function callSkippingSuccess(address target, bytes calldata data) external {
        (, result) = target.call(data);
    }

    function staticCall(address target, bytes calldata data) external {
        (ok, result) = target.staticcall(data);
    }

    function delegateCall(address target, bytes calldata data) external {
        (ok, result) = target.delegatecall(data);
    }
}
