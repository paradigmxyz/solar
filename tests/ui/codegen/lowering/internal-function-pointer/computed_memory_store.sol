//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7
//@ run-call: runLocal() => 11

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

struct Holder {
    function(Pair memory) internal pure returns (uint256) callback;
}

contract C {
    function run() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        Holder memory holder;
        assembly {
            mstore(holder, raw)
        }
        return holder.callback(Pair(3, 4));
    }

    // The pointer is written to scratch space and read back as a local.
    function runLocal() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        function(Pair memory) internal pure returns (uint256) typed;
        assembly {
            mstore(0, raw)
            typed := mload(0)
        }
        return typed(Pair(5, 6));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
