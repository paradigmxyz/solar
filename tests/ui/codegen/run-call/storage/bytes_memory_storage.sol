//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copy => 101
//@ run-call: dirty => 513
// ported-from: test/libsolidity/semanticTests/array/copying/bytes_memory_to_storage.sol
// ported-from: test/libsolidity/semanticTests/array/copying/dirty_memory_bytes_to_storage_copy.sol

contract BytesMemoryStorage {
    bytes stored;

    function copy() external returns (uint256) {
        bytes memory data = "abcd";
        stored = data;
        return stored.length + uint8(stored[0]);
    }

    function dirty() external returns (uint256) {
        bytes memory data = new bytes(3);
        assembly {
            mstore(add(data, 32), not(0))
        }
        stored = data;
        return stored.length + uint8(stored[0]) + uint8(stored[2]);
    }
}
