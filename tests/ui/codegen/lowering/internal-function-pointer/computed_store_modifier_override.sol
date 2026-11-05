//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: C::run() => 7

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

contract Base {
    function(Pair memory) internal pure returns (uint256) stored;
    uint256 internal calls;

    modifier guard(function(MemoryPointer) internal pure returns (uint256) raw, uint256 count)
        virtual
    {
        _;
    }

    // The named arguments follow the names of `Base.guard`, which the override renames.
    function run() external guard({raw: sum, count: 1}) returns (uint256) {
        calls += 1;
        return stored(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}

contract C is Base {
    modifier guard(function(MemoryPointer) internal pure returns (uint256) count, uint256 raw)
        override
    {
        assembly {
            sstore(0, count)
        }
        _;
    }
}
