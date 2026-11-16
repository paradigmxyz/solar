//@ codegen-matrix: standard
//@ run-call-fail: run() => Panic(0x51)

// An internal function pointer that assembly stores into a variable of another function type
// panics with `Panic(0x51)` when called; solc returns 7. See CODEGEN-021 in
// docs/SOLC_DIVERGENCE.md.
type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

contract C {
    function(Pair memory) internal pure returns (uint256) stored;

    function run() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        assembly {
            sstore(0, raw)
        }
        return stored(Pair(3, 4));
    }

    function sum(MemoryPointer pointer) internal pure returns (uint256 value) {
        assembly {
            value := add(mload(pointer), mload(add(pointer, 0x20)))
        }
    }
}
