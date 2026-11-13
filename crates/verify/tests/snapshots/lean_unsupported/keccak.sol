// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: call of `keccak256` not exported
    /// @custom:ensures keccak256(abi.encode(x)) != 0
    function f() external {}
}
