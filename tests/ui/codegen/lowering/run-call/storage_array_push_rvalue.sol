//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: wordRvalue 0xff => 0xff, 1
//@ run-call: wordRvalue 0 => 0, 1
//@ run-call: packedRvalue 0x0302 => 2, 3, 2
//@ run-call: boolRvalue 0xff00 => false, true, 2
//@ run-call: tagRvalue 0x1122334455667788 => 0x55667788, 0x11223344, 2
//@ run-call: enumRvalue 0x0203 => 3, 2, 2
//@ run-call: signedRvalue 0xffffffffffffffffffffffffffffffff00000000000000000000000000000005 => 5, -1, 2
//@ run-call: addressRvalue => 0xffffffffffffffffffffffffffffffffffffffff, 1
//@ run-call: unwrapRvalue 0x0000000000000009 => 9, 0, 2
//@ run-call: cleanRvalue => 0, 0, 2
//@ run-call: bareRvalue 0x2a => 0x2a, 1
//@ run-call: enumBareRvalue 7 => 7, 1
//@ run-call: enumTupleBare 7 => 7, 2
//@ run-call: enumTernaryBare 7, true => 7, 1
//@ run-call-fail: enumInvalidRvalue 7 => Panic(0x21)
//@ run-call: nestedInStmtRvalue 0x2a => 0x2a, 1
//@ run-call: memberRvalue 5 => 5, 6, 1
//@ run-call: pointerRvalue 0x2a => 0x2a, 1
//@ run-call: lvalueRvalue 0xff, 9 => 9, 1
//@ run-call: compoundRvalue 5, 9 => 14, 1
//@ run-call: structRvalue 41 => 41, 42, 2
//@ run-call: nestedRvalue 43 => 2, 43, 1

// `a.push()` appends an element without writing it, and its value is a
// reference to the new element, so reading that value yields whatever the slot
// already held. Neither compiler clears the element on push, so the content is
// only ever nonzero when inline assembly wrote through the array's data area,
// which every dirtying function below does before pushing. Value-typed
// elements read the element back, packed ones through their own byte range;
// aggregates keep binding a reference instead. An expression statement
// observes nothing, so the pushes it discards skip the read, which for an enum
// element also skips the range check that read would run.

contract StorageArrayPushRvalue {
    enum Direction {
        Up,
        Down,
        Left,
        Right
    }

    struct Entry {
        uint256 value;
    }

    struct Holder {
        uint256[] arr;
    }

    type Amount is uint64;

    uint256[] private words;
    uint8[] private smalls;
    bool[] private flags;
    bytes4[] private tags;
    Direction[] private directions;
    int128[] private signeds;
    address[] private addresses;
    Amount[] private amounts;
    Entry[] private entries;
    uint256[][] private nested;
    Holder private holder;
    mapping(uint256 => uint256[]) private byKey;
    uint256 private sink;

    function wordRvalue(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        return (words.push(), words.length);
    }

    function packedRvalue(uint256 dirty) external returns (uint8, uint8, uint256) {
        assembly {
            mstore(0, smalls.slot)
            sstore(keccak256(0, 32), dirty)
        }
        uint8 first = smalls.push();
        uint8 second = smalls.push();
        return (first, second, smalls.length);
    }

    function boolRvalue(uint256 dirty) external returns (bool, bool, uint256) {
        assembly {
            mstore(0, flags.slot)
            sstore(keccak256(0, 32), dirty)
        }
        bool first = flags.push();
        bool second = flags.push();
        return (first, second, flags.length);
    }

    function tagRvalue(uint256 dirty) external returns (bytes4, bytes4, uint256) {
        assembly {
            mstore(0, tags.slot)
            sstore(keccak256(0, 32), dirty)
        }
        bytes4 first = tags.push();
        bytes4 second = tags.push();
        return (first, second, tags.length);
    }

    function enumRvalue(uint256 dirty) external returns (Direction, Direction, uint256) {
        assembly {
            mstore(0, directions.slot)
            sstore(keccak256(0, 32), dirty)
        }
        Direction first = directions.push();
        Direction second = directions.push();
        return (first, second, directions.length);
    }

    function signedRvalue(uint256 dirty) external returns (int128, int128, uint256) {
        assembly {
            mstore(0, signeds.slot)
            sstore(keccak256(0, 32), dirty)
        }
        int128 first = signeds.push();
        int128 second = signeds.push();
        return (first, second, signeds.length);
    }

    // One address per slot, so the element read has to mask the upper 12 bytes
    // the dirty word leaves in place.
    function addressRvalue() external returns (address, uint256) {
        assembly {
            mstore(0, addresses.slot)
            sstore(keccak256(0, 32), not(0))
        }
        return (addresses.push(), addresses.length);
    }

    function unwrapRvalue(uint256 dirty) external returns (uint64, uint64, uint256) {
        assembly {
            mstore(0, amounts.slot)
            sstore(keccak256(0, 32), dirty)
        }
        uint64 first = Amount.unwrap(amounts.push());
        uint64 second = Amount.unwrap(amounts.push());
        return (first, second, amounts.length);
    }

    function cleanRvalue() external returns (uint256, uint256, uint256) {
        return (words.push(), words.push(), words.length);
    }

    // A bare `a.push();` never reads the element, and leaves it alone.
    function bareRvalue(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        words.push();
        return (words[words.length - 1], words.length);
    }

    // Reading a dirtied enum element runs the same range check as any other
    // storage read, so an out-of-range value panics like it does in solc.
    function enumInvalidRvalue(uint256 dirty) external returns (Direction, uint256) {
        assembly {
            mstore(0, directions.slot)
            sstore(keccak256(0, 32), dirty)
        }
        return (directions.push(), directions.length);
    }

    // A bare push never reads the element, so the range check never runs and an
    // out-of-range value survives untouched, as it does in solc.
    function enumBareRvalue(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, directions.slot)
            sstore(keccak256(0, 32), dirty)
        }
        directions.push();
        uint256 raw;
        assembly {
            mstore(0, directions.slot)
            raw := and(sload(keccak256(0, 32)), 0xff)
        }
        return (raw, directions.length);
    }

    // A discarded statement hands the discard down to the tuple components and
    // conditional branches that only forward its value, so these pushes do not
    // read the element either.
    function enumTupleBare(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, directions.slot)
            sstore(keccak256(0, 32), dirty)
        }
        (directions.push(), directions.push());
        uint256 raw;
        assembly {
            mstore(0, directions.slot)
            raw := and(sload(keccak256(0, 32)), 0xff)
        }
        return (raw, directions.length);
    }

    function enumTernaryBare(uint256 dirty, bool flag) external returns (uint256, uint256) {
        assembly {
            mstore(0, directions.slot)
            sstore(keccak256(0, 32), dirty)
        }
        flag ? directions.push() : directions.push();
        uint256 raw;
        assembly {
            mstore(0, directions.slot)
            raw := and(sload(keccak256(0, 32)), 0xff)
        }
        return (raw, directions.length);
    }

    // Only the statement's own expression is discarded: the push nested in this
    // assignment still reads the element, which is the shape the finding
    // reported.
    function nestedInStmtRvalue(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        sink = words.push();
        return (sink, words.length);
    }

    // Struct member and mapping value bases reach the element through the same
    // access the growth returns.
    function memberRvalue(uint256 dirty) external returns (uint256, uint256, uint256) {
        assembly {
            mstore(0, holder.slot)
            sstore(keccak256(0, 32), dirty)
            mstore(0, 1)
            mstore(32, byKey.slot)
            mstore(0, keccak256(0, 64))
            sstore(keccak256(0, 32), add(dirty, 1))
        }
        return (holder.arr.push(), byKey[1].push(), holder.arr.length);
    }

    function pushThrough(uint256[] storage array) internal returns (uint256) {
        return array.push();
    }

    function pointerRvalue(uint256 dirty) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        return (pushThrough(words), words.length);
    }

    function lvalueRvalue(uint256 dirty, uint256 value) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        words.push() = value;
        return (words[words.length - 1], words.length);
    }

    function compoundRvalue(uint256 dirty, uint256 value) external returns (uint256, uint256) {
        assembly {
            mstore(0, words.slot)
            sstore(keccak256(0, 32), dirty)
        }
        words.push() += value;
        return (words[words.length - 1], words.length);
    }

    // Aggregate elements have no value to read: the push binds a reference.
    function structRvalue(uint256 value) external returns (uint256, uint256, uint256) {
        Entry storage entry = entries.push();
        entry.value = value;
        entries.push().value = value + 1;
        return (entries[0].value, entries[1].value, entries.length);
    }

    function nestedRvalue(uint256 value) external returns (uint256, uint256, uint256) {
        uint256[] storage inner = nested.push();
        inner.push();
        inner.push() = value;
        return (inner.length, nested[0][1], nested.length);
    }
}
