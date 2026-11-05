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
    // Assembly points `alias_` at `holder` by address, and Solidity then stores through it.
    function run() external pure returns (uint256) {
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
        alias_.callback = typed;
        return holder.callback(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
