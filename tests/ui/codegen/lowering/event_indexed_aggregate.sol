//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventIndexedAggregate {
    event IndexedArray(uint256[2] indexed values);
    event IndexedDynamicArray(uint256[] indexed values);

    // CHECK-LABEL: fn @emitArray{{[( ]}}
    // CHECK: abi_encode [array<2, word>]
    // CHECK: keccak256
    // CHECK: log2
    function emitArray(uint256[2] memory values) external {
        emit IndexedArray(values);
    }

    // CHECK-LABEL: fn @emitDynamicArray{{[( ]}}
    // CHECK-DAG: memory_object_load_element memoryarray<1>
    // CHECK-DAG: keccak256_bytes
    // CHECK-DAG: log2
    function emitDynamicArray(uint256[] memory values) external {
        emit IndexedDynamicArray(values);
    }
}
