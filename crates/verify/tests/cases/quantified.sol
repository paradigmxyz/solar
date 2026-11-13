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

/// A pair-style lock and a hashed selector under a quantified invariant: the mutated lock must
/// come back with a counterexample, not unknown.
/// @custom:invariant forall address s :: allowance[address(0)][s] == 0
contract Locked {
    bytes4 private constant SELECTOR = bytes4(keccak256(bytes("transfer(address,uint256)")));
    uint256 private unlocked = 1;
    mapping(address => mapping(address => uint256)) public allowance;

    modifier lock() {
        require(unlocked == 1);
        unlocked = 0;
        _;
        unlocked = 2;
    }

    modifier goodLock() {
        require(unlocked == 1);
        unlocked = 0;
        _;
        unlocked = 1;
    }

    /// @custom:ensures s == 0xa9059cbb
    function ok_selector() external pure returns (bytes4 s) {
        s = SELECTOR;
    }

    /// Fixed bytes are left-aligned: narrowing keeps the high bytes, widening pads on the right.
    /// @custom:ensures uint256(w) == uint256(uint32(b)) * 2 ** 224
    /// @custom:ensures uint16(h) == uint32(b) / 2 ** 16
    function ok_widths(bytes4 b) external pure returns (bytes32 w, bytes2 h) {
        w = bytes32(b);
        h = bytes2(b);
    }

    // expect: counterexample
    /// @custom:ensures uint16(h) == uint32(b) % 2 ** 16
    function bad_narrow(bytes4 b) external pure returns (bytes2 h) {
        h = bytes2(b);
    }

    // expect: counterexample
    /// @custom:ensures unlocked == 1
    function bad_lock(address token, address to, uint256 v) external lock {
        (bool ok,) = token.call(abi.encodeWithSelector(SELECTOR, to, v));
        require(ok);
    }

    /// @custom:ensures unlocked == 1
    function ok_lock(address token, address to, uint256 v) external goodLock {
        (bool ok,) = token.call(abi.encodeWithSelector(SELECTOR, to, v));
        require(ok);
    }
}
