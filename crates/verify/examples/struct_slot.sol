// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;
contract C {
    struct S { uint120 a; uint120 b; bool live; }
    mapping(uint256 => S) m;
    uint256 private constant FRESH = 1 << 240;
    /// @custom:ensures m[k].a == 0 && m[k].b == 0 && m[k].live
    function f(uint256 k) public {
        S storage s = m[k];
        assembly ("memory-safe") { sstore(s.slot, FRESH) }
    }
    /// @custom:ensures m[k].a == 5 && m[k].b == 7 && !m[k].live
    function g(uint256 k) public {
        S storage s = m[k];
        uint256 w = 5 | (7 << 120);
        assembly ("memory-safe") { sstore(s.slot, w) }
    }
    /// @custom:ensures result == (m[k].live ? 1 << 240 : 0) + (uint256(m[k].b) << 120) + m[k].a
    function h(uint256 k) public view returns (uint256 result) {
        S storage s = m[k];
        assembly ("memory-safe") { result := sload(s.slot) }
    }
    /// @custom:ensures m[k].live == false
    function bad(uint256 k) public {
        S storage s = m[k];
        assembly ("memory-safe") { sstore(s.slot, FRESH) }
    }
}
