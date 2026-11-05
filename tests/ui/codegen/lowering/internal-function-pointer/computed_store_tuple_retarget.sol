//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7

type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

struct Holder {
    function(Pair memory) internal pure returns (uint256) callback;
}

contract C {
    function(Pair memory) internal pure returns (uint256) stored;
    Holder other;

    // A multiple assignment points `holder` at slot 0.
    function run() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        function(Pair memory) internal pure returns (uint256) typed;
        assembly {
            typed := raw
        }
        Holder storage holder = other;
        assembly {
            function slots() -> a, b {
                a := 0
                b := 0
            }
            let ignored
            holder.slot, ignored := slots()
        }
        holder.callback = typed;
        return stored(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
