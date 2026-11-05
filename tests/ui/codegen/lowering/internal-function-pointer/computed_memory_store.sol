//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run() => 7
//@ run-call: runWord() => 11

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

    // The holder is the last allocation, and assembly reaches it only by address.
    function run() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        Holder memory holder;
        assembly {
            mstore(sub(mload(0x40), 0x20), raw)
        }
        return holder.callback(Pair(3, 4));
    }

    // The pointer passes through a word before a second block stores it.
    function runWord() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        uint256 word;
        assembly {
            word := raw
        }
        assembly {
            sstore(0, word)
        }
        return stored(Pair(5, 6));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
