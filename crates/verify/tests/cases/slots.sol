// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// .slot and .offset of state variables in assembly, enums assigned in assembly and byte with a
// symbolic index: every ok_* must verify, every bad_* must fail.
contract Slots {
    enum Kind {
        A,
        B,
        C,
        D,
        E
    }

    mapping(address => uint256) internal counters;
    mapping(address => mapping(uint256 => uint256)) internal nested;
    mapping(address => int256) internal signedOf;
    mapping(address => uint64) internal narrowOf;
    uint256 internal word;
    uint64 internal small;
    uint64 internal small2;

    /// Seaport's counter increment.
    /// @custom:ensures counters[msg.sender] == old(counters[msg.sender]) + 1
    function ok_mapping_slot() public {
        require(counters[msg.sender] < 1000);
        assembly {
            mstore(0, caller())
            mstore(0x20, counters.slot)
            let p := keccak256(0, 0x40)
            sstore(p, add(sload(p), 1))
        }
    }

    /// @custom:ensures counters[msg.sender] == old(counters[msg.sender]) + 2
    function bad_mapping_slot() public {
        require(counters[msg.sender] < 1000);
        assembly {
            mstore(0, caller())
            mstore(0x20, counters.slot)
            let p := keccak256(0, 0x40)
            sstore(p, add(sload(p), 1))
        }
    }

    function ok_mapping_read(address a, uint256 v) public {
        counters[a] = v;
        uint256 r;
        assembly {
            mstore(0, a)
            mstore(0x20, counters.slot)
            r := sload(keccak256(0, 0x40))
        }
        assert(r == v);
    }

    function ok_mapping_other_key(address a, address b, uint256 v) public {
        require(a != b);
        uint256 before = counters[b];
        assembly {
            mstore(0, a)
            mstore(0x20, counters.slot)
            sstore(keccak256(0, 0x40), v)
        }
        assert(counters[b] == before && counters[a] == v);
    }

    function bad_mapping_other_key(address a, address b, uint256 v) public {
        uint256 before = counters[b];
        assembly {
            mstore(0, a)
            mstore(0x20, counters.slot)
            sstore(keccak256(0, 0x40), v)
        }
        assert(counters[b] == before);
    }

    /// A key no address equals names a raw slot, not an entry.
    function ok_mapping_wide_key(address a, uint256 v) public {
        uint256 before = counters[a];
        assembly {
            mstore(0, or(a, shl(160, 1)))
            mstore(0x20, counters.slot)
            sstore(keccak256(0, 0x40), v)
        }
        assert(counters[a] == before);
    }

    function ok_nested(address a, uint256 i, uint256 v) public {
        assembly {
            mstore(0, a)
            mstore(0x20, nested.slot)
            let inner := keccak256(0, 0x40)
            mstore(0, i)
            mstore(0x20, inner)
            sstore(keccak256(0, 0x40), v)
        }
        assert(nested[a][i] == v);
    }

    function bad_nested(address a, uint256 i, uint256 j, uint256 v) public {
        assembly {
            mstore(0, a)
            mstore(0x20, nested.slot)
            let inner := keccak256(0, 0x40)
            mstore(0, i)
            mstore(0x20, inner)
            sstore(keccak256(0, 0x40), v)
        }
        assert(nested[a][j] == v);
    }

    function ok_signed(address a, int256 v) public {
        assembly {
            mstore(0, a)
            mstore(0x20, signedOf.slot)
            sstore(keccak256(0, 0x40), v)
        }
        assert(signedOf[a] == v);
    }

    function ok_word(uint256 v) public {
        assembly {
            sstore(word.slot, v)
        }
        assert(word == v);
    }

    function ok_offset() public view {
        uint256 o;
        assembly {
            o := counters.offset
        }
        assert(o == 0);
    }

    /// A function summarized by its contract may write what its assembly reaches.
    /// @custom:ensures true
    function bumpRaw() internal {
        assembly {
            mstore(0, caller())
            mstore(0x20, counters.slot)
            let p := keccak256(0, 0x40)
            sstore(p, add(sload(p), 1))
        }
    }

    function bad_summary_havoc() public {
        uint256 before = counters[msg.sender];
        bumpRaw();
        assert(counters[msg.sender] == before);
    }

    // expect: may share its slot
    function bad_packed_slot() public view returns (uint256 s) {
        assembly {
            s := small.slot
        }
    }

    // expect: is not one whole word
    function bad_narrow_value(address a) public {
        assembly {
            mstore(0, a)
            mstore(0x20, narrowOf.slot)
            sstore(keccak256(0, 0x40), 1)
        }
    }

    // expect: derived from a
    function bad_slot_arithmetic(uint256 v) public {
        assembly {
            sstore(add(word.slot, 1), v)
        }
    }

    // ---------------------------------------------------------------- enums
    function ok_enum_masked(uint256 x) public pure returns (Kind k) {
        assembly {
            k := and(x, 3)
        }
        assert(uint8(k) < 4);
    }

    // expect: outside enum
    function bad_enum_wide(uint256 x) public pure returns (Kind k) {
        assembly {
            k := shr(2, x)
        }
    }

    // ---------------------------------------------------------------- byte
    function ok_byte_symbolic(uint256 i) public pure {
        require(i < 6);
        uint256 b;
        assembly {
            b := byte(i, 0x0203020301010000000000000000000000000000000000000000000000000000)
        }
        assert(i < 4 ? b == 2 + (i % 2) : b == 1);
    }

    function ok_byte_far(uint256 i, uint256 x) public pure {
        require(i >= 32);
        uint256 b;
        assembly {
            b := byte(i, x)
        }
        assert(b == 0);
    }

    function bad_byte_symbolic(uint256 i) public pure {
        require(i < 7);
        uint256 b;
        assembly {
            b := byte(i, 0x0203020301010000000000000000000000000000000000000000000000000000)
        }
        assert(b >= 1);
    }
}
