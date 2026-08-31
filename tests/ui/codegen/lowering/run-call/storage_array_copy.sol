//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copyState => 1, 3
//@ run-call: copyReference => 4, 2
//@ run-call: copyAggregate => 7, 9, 1
//@ run-call-fail: EnumStorageCopy::smallInvalid() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: EnumStorageCopy::largeValid() => 2
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_storage_to_memory.sol

contract C {
    uint[] a;

    function copyState() public returns (uint, uint) {
        a.push(1);
        a.push(0);
        a.push(0);
        uint[] memory b = a;
        return (b[0], b.length);
    }

    function copyReference() public returns (uint, uint) {
        a.push(4);
        a.push(0);
        uint[] storage r = a;
        uint[] memory b = r;
        return (b[0], b.length);
    }
}

contract AggregateStorageCopy {
    struct Pair {
        uint x;
        uint y;
    }

    Pair[] pairs;

    function copyAggregate() public returns (uint, uint, uint) {
        pairs.push();
        pairs[0].x = 7;
        pairs[0].y = 9;
        Pair[] memory copied = pairs;
        return (copied[0].x, copied[0].y, copied.length);
    }
}

contract EnumStorageCopy {
    enum Small { Zero, One }
    enum Large { Zero, One, Two }

    Small[] small;
    Large[] large;

    function smallInvalid() public returns (uint256) {
        assembly ("memory-safe") {
            sstore(small.slot, 1)
            mstore(0, small.slot)
            sstore(keccak256(0, 32), 2)
        }
        Small[] memory copied = small;
        return uint256(copied[0]);
    }

    function largeValid() public returns (uint256) {
        assembly ("memory-safe") {
            sstore(large.slot, 1)
            mstore(0, large.slot)
            sstore(keccak256(0, 32), 2)
        }
        Large[] memory copied = large;
        return uint256(copied[0]);
    }
}
