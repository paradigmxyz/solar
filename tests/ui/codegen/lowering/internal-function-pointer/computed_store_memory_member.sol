//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 11

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

struct Holder {
    function(Pair memory) internal pure returns (uint256) callback;
}

struct Box {
    Holder inner;
}

contract C {
    // The store goes through a reference loaded from the assigned reference.
    function run() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        function(Pair memory) internal pure returns (uint256) typed;
        assembly {
            typed := raw
        }
        Box memory other;
        Holder memory holder;
        Box memory box = Box(holder);
        assembly {
            other := sub(mload(0x40), 0x20)
        }
        Holder memory inner = other.inner;
        inner.callback = typed;
        return holder.callback(Pair(5, 6));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
