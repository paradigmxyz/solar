// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract TransferTransient {
    uint256 transient t;
    uint256 s;

    function pokeT() external { t = t + 1; }
    function pokeS() external { s = s + 1; }

    /// A 2300-gas stipend cannot SSTORE (EIP-2200 sentry), so s is unchanged: true.
    /// @custom:ensures s == old(s)
    function ok_storage(address payable to) external {
        to.transfer(1);
    }

    /// TSTORE has no stipend sentry: the recipient can reenter pokeT() with 2300 gas. False.
    /// @custom:ensures t == old(t)
    function bad_transient(address payable to) external {
        to.transfer(1);
    }

    /// Same with send.
    /// @custom:ensures t == old(t)
    function bad_transient_send(address payable to) external {
        to.send(1);
    }
}
