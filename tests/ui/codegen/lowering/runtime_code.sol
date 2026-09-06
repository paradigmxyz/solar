//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RuntimeCodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract RuntimeCode {
    // CHECK-LABEL: @module RuntimeCode{{$}}
    // CHECK-LABEL: fn @runtime{{[( ]}}
    // CHECK: [[OBJECT:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[OBJECT]], 63
    // CHECK: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[OBJECT]]
    // The small runtime object is stored inline, with a zero-padded final word.
    // CHECK: mstore [[DATA]], 0x{{[0-9a-f]+}}
    // CHECK: [[TAIL:v[0-9]+]] = add [[DATA]], 32
    // CHECK: mstore [[TAIL]], 0x{{[0-9a-f]+}}00
    // CHECK-NOT: data_copy
    // CHECK: [[RESULT:v[0-9]+]] = memory_object_len memorybytes, [[OBJECT]]
    // CHECK: ret [[RESULT]]
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
