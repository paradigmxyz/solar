// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TupleSource {
    error Failed(uint256 code);

    function fail() external pure {
        revert Failed(7);
    }

    function echo(uint256 value) external pure returns (uint256) {
        return value;
    }

    function pair() external pure returns (uint256, bytes memory) {
        return (5, hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122");
    }

    fallback(bytes calldata) external returns (bytes memory) {
        return hex"010203";
    }

    function text() external pure returns (string memory, uint256) {
        return ("a string that is longer than thirty-one bytes", 9);
    }
}

/// Assigns call results and other tuples straight to storage targets.
contract TupleStorageAssign {
    struct Holder {
        uint256 id;
        bytes data;
    }

    bool public ok;
    bytes public result;
    uint256 public number;
    string public message;
    Holder public holder;
    mapping(uint256 => bytes) public byKey;

    function call(address target, bytes calldata data) external {
        (ok, result) = target.call(data);
    }

    function callIntoFields(address target, bytes calldata data) external {
        (holder.id, holder.data) = (1, "");
        (ok, holder.data) = target.call(data);
        (, byKey[4]) = target.staticcall(data);
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

    function decode(bytes calldata data) external {
        (number, result) = abi.decode(data, (uint256, bytes));
    }

    function externalPair(TupleSource source) external {
        (number, result) = source.pair();
    }

    function externalText(TupleSource source) external {
        (message, number) = source.text();
    }
}
