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
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 0, 0x{{[0-9a-f]+}}
    // CHECK: memory_object_store_word memorybytes, {{v[0-9]+}}, 32, 0x{{[0-9a-f]+}}
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
