// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// --spec-check: trusted contracts tested against their code. Each target named ok_* must hold
// and each bad_* must be reported violated, with a counterexample.

interface IPool {
    /// @custom:requires amount > 0
    /// @custom:ensures result <= amount
    function bad_quote(uint256 amount) external returns (uint256 result);

    /// @custom:ensures result == amount / 2
    function ok_half(uint256 amount) external returns (uint256 result);
}

contract Pool is IPool {
    uint256 public fee;

    function bad_quote(uint256 amount) external view returns (uint256 result) {
        // The fee is added rather than taken off.
        result = amount + (fee % 100);
    }

    function ok_half(uint256 amount) external pure returns (uint256 result) {
        result = amount / 2;
    }
}

contract Vault {
    mapping(address => uint256) public balances;
    uint256 public total;

    /// @custom:trusted
    /// @custom:requires x < 2 ** 128
    /// @custom:ensures result == x + 1
    function bad_inc(uint256 x) internal pure returns (uint256 result) {
        // Off by one from the spec at x = 7.
        result = x == 7 ? x : x + 1;
    }

    /// @custom:trusted
    /// @custom:ensures result == (a > b ? a : b)
    function ok_max(uint256 a, uint256 b) internal pure returns (uint256 result) {
        result = a >= b ? a : b;
    }

    /// @custom:trusted
    /// @custom:requires balances[msg.sender] >= amount
    /// @custom:ensures balances[msg.sender] == old(balances[msg.sender]) - amount
    /// @custom:ensures total == old(total)
    function ok_burnLocal(uint256 amount) public {
        balances[msg.sender] -= amount;
    }

    /// @custom:trusted
    /// @custom:succeeds-if amount <= balances[msg.sender]
    function bad_withdraw(uint256 amount) external {
        // Reverts on the full balance, which the succeeds-if allows.
        require(amount < balances[msg.sender]);
        balances[msg.sender] -= amount;
    }

    /// @custom:trusted
    /// @custom:requires x > 2 ** 256
    /// @custom:ensures result == 0
    function ok_vacuous(uint256 x) internal pure returns (uint256 result) {
        result = x;
    }

    function use(uint256 x) external pure returns (uint256) {
        return bad_inc(x) + ok_max(x, 1) + ok_vacuous(x);
    }
}

// Checked against the built-in summary, which floors: this one rounds up.
library Math {
    function mulDiv(uint256 x, uint256 y, uint256 denominator) internal pure returns (uint256) {
        return (x * y + denominator - 1) / denominator;
    }
}
