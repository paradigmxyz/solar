//@ codegen-matrix: standard
//@ run-call: run() => 7

// The same store works when the variable has the pointer's own function type; compare
// `internal_function_pointer_computed_storage_store.sol` and CODEGEN-021 in
// docs/SOLC_DIVERGENCE.md.
type MemoryPointer is uint256;

contract C {
    uint256 pad;
    function(MemoryPointer) internal pure returns (uint256) stored;

    function run() external returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) raw = sum;
        assembly {
            sstore(1, raw)
        }
        return stored(MemoryPointer.wrap(0x80));
    }

    function sum(MemoryPointer) internal pure returns (uint256 value) {
        value = 7;
    }
}
