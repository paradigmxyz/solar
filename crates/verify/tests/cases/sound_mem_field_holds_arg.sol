// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract A1 {
    struct Q { uint256[] xs; }

    function _f(uint256[] memory a, Q memory q) internal pure returns (uint256) {
        q.xs[0] = 5;
        return a[0];
    }

    function _g(uint256[] memory a, Q memory q) internal pure returns (uint256) {
        q.xs[0] = 5;
        return a.length;
    }

    // q.xs and a are one array: _f writes 5 through q.xs and reads it back through a.
    /// @custom:ensures result == 0
    function bad_field_then_call() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        Q memory q = Q(new uint256[](1));
        q.xs = a;
        result = _f(a, q);
    }

    /// Same, but the caller reads a[0] after the call.
    /// @custom:ensures result == 0
    function bad_field_then_call_read() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        Q memory q = Q(new uint256[](1));
        q.xs = a;
        _g(a, q);
        result = a[0];
    }

    // Refused too: _g writes q, which shares memory with a.
    /// @custom:ensures result == 1
    // expect: may share its memory with another reference
    function bad_field_then_call_len() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        Q memory q = Q(new uint256[](1));
        q.xs = a;
        result = _g(a, q);
    }
}
