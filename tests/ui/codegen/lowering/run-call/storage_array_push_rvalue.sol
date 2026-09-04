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
// aggregates keep binding a reference instead.

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
