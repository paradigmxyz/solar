// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Deep Nested Struct Tests
/// @notice Tests deeply nested structs (3+ levels) with storage-memory copying

struct Level1 {
    uint256 a;
}

struct Level2 {
    Level1 l1;
    uint256 b;
}

struct Level3 {
    Level2 l2;
    uint256 c;
}

contract DeepNested {
    Level3 public stored;

    /// @notice Set deeply nested struct value via memory, copy to storage
    function setDeep(uint256 val) public {
        Level3 memory l3;
        l3.l2.l1.a = val;
        l3.l2.b = val + 1;
        l3.c = val + 2;
        stored = l3;
    }

    /// @notice Get deeply nested value via storage-to-memory copy
    function getDeep() public view returns (uint256 a, uint256 b, uint256 c) {
        Level3 memory l3 = stored;
        return (l3.l2.l1.a, l3.l2.b, l3.c);
    }

    /// @notice Set individual fields in storage
    function setFields(uint256 a, uint256 b, uint256 c) public {
        stored.l2.l1.a = a;
        stored.l2.b = b;
        stored.c = c;
    }

    /// @notice Get individual fields from storage
    function getFields() public view returns (uint256 a, uint256 b, uint256 c) {
        return (stored.l2.l1.a, stored.l2.b, stored.c);
    }

    /// @notice Round-trip test: memory -> storage -> memory
    function roundTrip(uint256 a, uint256 b, uint256 c) public returns (uint256, uint256, uint256) {
        Level3 memory input;
        input.l2.l1.a = a;
        input.l2.b = b;
        input.c = c;
        stored = input;
        Level3 memory output = stored;
        return (output.l2.l1.a, output.l2.b, output.c);
    }
}
