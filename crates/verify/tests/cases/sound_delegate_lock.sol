// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// A delegatecall runs outside code on this contract's storage, so the lock does not stop it
/// writing x (or the lock itself, or sending ETH away).
/// @custom:reentrancy-guard locked
contract DelegateUnderLock {
    uint256 public x;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }

    /// @custom:ensures x == old(x)
    function bad_delegate(address t) external lock {
        (bool ok, ) = t.delegatecall("");
        require(ok);
    }

    /// @custom:ensures x == old(x)
    function ok_call(address t) external lock {
        (bool ok, ) = t.call("");
        require(ok);
    }
}
