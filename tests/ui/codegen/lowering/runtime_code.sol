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
    // CHECK: [[DATA:v[0-9]+]] = memory_object_data memorybytes, {{v[0-9]+}}
    // CHECK: mstore [[DATA]], 0x{{[0-9a-f]+}}
    // CHECK: [[SECOND:v[0-9]+]] = add [[DATA]], 32
    // CHECK: mstore [[SECOND]], 0x{{[0-9a-f]+}}
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
