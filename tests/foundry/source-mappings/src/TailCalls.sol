// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TailCalls {
    mapping(address => uint256) public balances;
    uint256 public total;

    error Zero();

    function twice(uint256 a, uint256 b) external returns (uint256) {
        return _mix(a, b) + _mix(b, a);
    }

    function mint(address to, uint256 value) external {
        _update(address(0), to, value);
    }

    function transfer(address from, address to, uint256 value) external {
        _transfer(from, to, value);
    }

    function transferTwice(address from, address to, uint256 value) external {
        _transfer(from, to, value);
        _transfer(to, from, value);
    }

    function lookup(uint256 a, uint256 b) external returns (uint256) {
        return _forward(a, b);
    }

    function lookupZero(uint256 b) external returns (uint256) {
        return _forward(0, b);
    }

    function _update(address from, address to, uint256 value) internal {
        if (from == address(0)) {
            total += value;
        } else {
            balances[from] -= value;
        }
        balances[to] += value;
        total ^= value * 3;
        total += uint160(from) + uint160(to);
    }

    function _mix(uint256 a, uint256 b) internal returns (uint256 r) {
        assembly {
            r := xor(a, shl(1, b))
            r := add(r, mul(a, 3))
            r := xor(r, shr(7, b))
        }
        total += r;
        total ^= b;
        total += a;
        total *= 3;
    }

    function _transfer(address from, address to, uint256 value) internal {
        if (from == address(0)) revert Zero();
        if (to == address(0)) revert Zero();
        _update(from, to, value);
    }

    function _forward(uint256 a, uint256 b) internal returns (uint256) {
        if (a == b) revert Zero();
        return _mix(a, b);
    }
}
