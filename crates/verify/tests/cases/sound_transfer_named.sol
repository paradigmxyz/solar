// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IT {
    function transfer(address to, uint256 a) external returns (bool);
    function pay(address to, uint256 a) external returns (bool);
}

/// An interface function named transfer is a full external call that can reenter.
contract TransferNamed {
    uint256 public x;

    /// @custom:ensures true
    function _send(IT t) internal { t.transfer(msg.sender, 1); }

    /// @custom:ensures true
    function _pay(IT t) internal { t.pay(msg.sender, 1); }

    function setX(uint256 v) external { x = v; }

    /// @custom:ensures x == old(x)
    function bad_summary(IT t) external { _send(t); }

    /// @custom:ensures x == old(x)
    function bad_summary_pay(IT t) external { _pay(t); }

    /// @custom:ensures x == old(x)
    function bad_loop(IT t, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            t.transfer(msg.sender, 1);
        }
    }

    /// @custom:ensures x == old(x)
    function bad_direct(IT t) external { t.transfer(msg.sender, 1); }
}
