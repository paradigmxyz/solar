// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Counterexamples under quantified contract invariants: z3's default core answers unknown
// ("incomplete quantifiers") on these bad_* functions; the verifier must still report a
// counterexample, and the ok_* ones must still verify.

/// @custom:invariant forall address s :: allowance[address(0)][s] == 0
contract Token {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    // expect: counterexample
    /// @custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + value
    function bad_transfer(address to, uint256 value) external returns (bool) {
        balanceOf[msg.sender] -= value;
        balanceOf[to] += value + 1;
        return true;
    }

    /// @custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + value
    /// @custom:ensures forall address a :: a != msg.sender && a != to ==> balanceOf[a] == old(balanceOf[a])
    function ok_transfer(address to, uint256 value) external returns (bool) {
        balanceOf[msg.sender] -= value;
        balanceOf[to] += value;
        return true;
    }

    // expect: counterexample
    function bad_approve(address spender, uint256 value) external {
        allowance[msg.sender][spender] = value;
        allowance[address(0)][spender] = value;
    }

    function ok_approve(address spender, uint256 value) external {
        require(msg.sender != address(0));
        allowance[msg.sender][spender] = value;
    }
}
