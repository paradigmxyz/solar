//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

contract C {
    function(Pair memory) internal pure returns (uint256) stored;
    // The initializer runs in the constructor, where assembly stores `sum` into `stored`.
    uint256 initialized = store(sum);

    function run() external view returns (uint256) {
        return stored(Pair(3, 4));
    }

    function store(function(MemoryPointer) internal pure returns (uint256) raw)
        internal
        returns (uint256)
    {
        assembly {
            sstore(0, raw)
        }
        return 1;
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
