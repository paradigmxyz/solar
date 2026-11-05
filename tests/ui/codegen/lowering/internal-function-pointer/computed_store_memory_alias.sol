//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: runAlias() => 11

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

struct Holder {
    function(Pair memory) internal pure returns (uint256) callback;
}

contract C {
    // The store goes through a parameter that aliases the assigned reference.
    function runAlias() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        function(Pair memory) internal pure returns (uint256) typed;
        assembly {
            typed := raw
        }
        Holder memory holder;
        Holder memory alias_;
        assembly {
            alias_ := sub(mload(0x40), 0x40)
        }
        put(alias_, typed);
        return holder.callback(Pair(5, 6));
    }

    function put(Holder memory target, function(Pair memory) internal pure returns (uint256) value)
        internal
        pure
    {
        target.callback = value;
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
