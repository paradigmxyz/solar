// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface I {
    function get() external returns (uint8);
    function get256() external returns (uint256);
    function poke() external;
}

contract TryLive {
    /// False: a callee returning 256 (or empty data) fails decoding in the caller, which catch does not catch.
    /// @custom:succeeds-if true
    function bad_decode(address a) external payable returns (uint8 r) {
        try I(a).get() returns (uint8 v) { r = v; } catch { r = 1; }
    }

    /// False: a callee whose fallback returns no data fails decoding in the caller.
    /// @custom:succeeds-if a.code.length > 0
    function bad_short(address a) external payable returns (uint256 r) {
        try I(a).get256() returns (uint256 v) { r = v; } catch { r = 1; }
    }

    /// False: a target without code reverts in the caller (extcodesize check), not caught.
    /// @custom:succeeds-if true
    function bad_nocode(address a) external payable {
        try I(a).poke() {} catch {}
    }

    /// True.
    /// @custom:succeeds-if true
    function ok_plain(uint256 x) external payable returns (uint256) {
        return x / 2;
    }

    /// Non-payable with msg.value > 0 reverts.
    /// @custom:succeeds-if true
    function nonpayable_plain(uint256 x) external returns (uint256) {
        return x / 2;
    }

    /// @custom:succeeds-if true
    function half(uint256 x) external pure returns (uint256) {
        return x / 2;
    }

    // This contract's own code runs and returns a well-formed word: only the callee's revert
    // (caught) can stop it.
    /// @custom:succeeds-if true
    function ok_try_self(uint256 x) external payable returns (uint256 r) {
        try this.half(x) returns (uint256 v) { r = v; } catch { r = 1; }
    }
}
