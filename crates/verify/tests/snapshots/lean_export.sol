// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/// Every form --emit-lean translates.
/// @custom:ghost uint256 total
/// @custom:invariant count <= 1000
/// @custom:invariant total == count
contract Export {
    /// @custom:invariant lo <= hi
    struct Range {
        uint64 lo;
        uint64 hi;
        bool open;
    }

    uint8 small;
    int16 signed16;
    address owner;
    uint256 count;
    mapping(address => uint256) balances;
    mapping(address => mapping(uint256 => bool)) flags;
    uint256[] items;
    uint8[] bytes8s;
    Range range;
    mapping(uint256 => Range) ranges;
    uint256 constant LIMIT = 1000;

    function sq(uint256 a) internal pure returns (uint256) {
        return a * a;
    }

    /// @custom:opaque
    function half(uint256 a) internal pure returns (uint256) {
        return a / 2;
    }

    /// @custom:requires count < LIMIT
    /// @custom:ensures count == old(count) + 1
    /// @custom:ensures balances[msg.sender] == old(balances[msg.sender]) + amount
    /// @custom:ensures total == old(total) + 1
    /// @custom:ensures n == count
    function deposit(uint256 amount) external returns (uint256 n) {
        balances[msg.sender] += amount;
        /// @custom:ghost total = total + 1
        count += 1;
        return count;
    }

    /// @custom:ensures forall uint256 i :: i < items.length ==> items[i] == old(items[i])
    /// @custom:ensures exists address a :: balances[a] > 0 <==> result
    /// @custom:ensures flags[who][k] ==> sq(k) >= k
    /// @custom:ensures half(k) <= k && (k >> 1) == half(k) && (k << 2) >= 0
    /// @custom:ensures bytes8s.length == old(bytes8s.length)
    /// @custom:ensures (bytes8s.length > 0 ? bytes8s[0] : 0) <= 255
    /// @custom:ensures range.lo <= range.hi && ranges[k].open == old(ranges[k].open)
    /// @custom:ensures small < 256 && signed16 >= -32768 && owner == owner
    /// @custom:ensures __time <= block.timestamp && this == this
    /// @custom:succeeds-if !(small > 3) && -signed16 <= 32768 && (count & 1) | 2 >= (count ^ 3) % 4 - 4
    function probe(address who, uint256 k) external view returns (bool) {
        return count > 0;
    }
}
