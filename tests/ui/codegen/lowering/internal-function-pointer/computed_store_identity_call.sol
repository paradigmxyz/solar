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
    // The identity precompile copies `source` to the last allocation, which assembly reaches
    // only by address.
    function run() external view returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        function(Pair memory) internal pure returns (uint256) typed;
        assembly {
            typed := raw
        }
        Holder memory source = Holder(typed);
        Holder memory target;
        assembly {
            pop(staticcall(gas(), 4, source, 0x20, sub(mload(0x40), 0x20), 0x20))
        }
        return target.callback(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
