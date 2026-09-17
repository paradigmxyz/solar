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
    // CHECK: [[DATA_PTR:v[0-9]+]] = memory_object_data memorybytes, {{v[0-9]+}}
    // CHECK: [[DATA:v[0-9]+]] = ptrtoint memptr [[DATA_PTR]] to i256
    // CHECK: mstore [[DATA]], 0x{{[0-9a-f]+}}
    // CHECK: [[OFFSET_DATA:v[0-9]+]] = ptrtoint memptr [[DATA_PTR]] to i256
    // CHECK: [[SECOND:v[0-9]+]] = add [[OFFSET_DATA]], 32
    // CHECK: mstore [[SECOND]], 0x{{[0-9a-f]+}}
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
