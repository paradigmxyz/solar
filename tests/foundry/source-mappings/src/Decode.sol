// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Decode {
    enum Op { A, B }
    uint256 public stored;

    function execute(uint256 amount, bytes calldata data) external returns (address, uint256) {
        (Op op, bytes memory rest) = abi.decode(data, (Op, bytes));
        if (op == Op.A) {
            (address who, uint256 got) = _a(amount, rest);
            return (who, got);
        }
        return (address(0), 0);
    }

    function _a(uint256 amount, bytes memory rest) internal returns (address, uint256) {
        (address who, uint256 extra) = abi.decode(rest, (address, uint256));
        stored = amount + extra;
        return (who, stored);
    }
}
