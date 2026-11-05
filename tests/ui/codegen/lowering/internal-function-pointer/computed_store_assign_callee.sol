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

    // The callee is an assignment expression.
    function run() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        assembly {
            sstore(0, raw)
        }
        function(Pair memory) internal pure returns (uint256) local;
        return (local = stored)(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
