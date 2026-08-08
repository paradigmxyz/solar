//@compile-flags: -Zcodegen -O none -Zdump=mir
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
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 32, 0x5f80fd5b3660049010156046576042565b{{.*}}
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 64, 0x80f35b5f80fd5b60305600000000000000000000000000000000000000000000
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
