//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RuntimeCodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract RuntimeCode {
    // CHECK-LABEL: fn @runtime{{[( ]}}
    // CHECK: alloc memorybytes
    // CHECK: set_memory_object_len memorybytes
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 0, 0x60a060405234601f575f3560e01c80633fa4f24514601b57601f565b6023565b
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 32, 0x5f80fd5b60076080526020608001608090036080f300{{.*}}
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
