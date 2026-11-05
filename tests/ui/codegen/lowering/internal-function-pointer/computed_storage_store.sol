//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7
//@ run-call: runSlot() => 11
//@ run-call: runMapping 5 => 9

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

contract C {
    function(Pair memory) internal pure returns (uint256) stored;
    function(Pair memory) internal pure returns (uint256)[2] table;
    mapping(uint256 => function(Pair memory) internal pure returns (uint256)) map;

    function run() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        assembly {
            sstore(0, raw)
        }
        return stored(Pair(3, 4));
    }

    // Function pointers pack four to a slot, so element 1 sits 64 bits up.
    function runSlot() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        uint256 slot;
        assembly {
            slot := table.slot
            sstore(slot, shl(64, raw))
        }
        return table[1](Pair(5, 6));
    }

    function runMapping(uint256 key) external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        assembly {
            mstore(0, key)
            mstore(0x20, map.slot)
            sstore(keccak256(0, 0x40), raw)
        }
        return map[key](Pair(4, 5));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
