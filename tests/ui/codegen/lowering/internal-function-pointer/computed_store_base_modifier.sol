//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

abstract contract Base {
    function(Pair memory) internal pure returns (uint256) stored;

    constructor(function(MemoryPointer) internal pure returns (uint256) raw) {
        assembly {
            sstore(0, raw)
        }
    }
}

// The base constructor takes the pointer from a constructor modifier.
contract C is Base {
    constructor() Base(sum) {}

    function run() external view returns (uint256) {
        return stored(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
