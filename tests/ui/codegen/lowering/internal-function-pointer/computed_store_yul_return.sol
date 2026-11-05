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
    // Function pointers pack into slots, so this keeps `source` out of slot 0.
    uint256 padding;
    function(MemoryPointer) internal pure returns (uint256) source = sum;

    // The stored word is the return of a Yul function that loads it.
    function run() external returns (uint256) {
        assembly {
            function read() -> value {
                value := sload(source.slot)
            }
            sstore(0, read())
        }
        return stored(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
