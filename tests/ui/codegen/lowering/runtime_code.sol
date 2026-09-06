//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RuntimeCodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract RuntimeCode {
    // CHECK-LABEL: @module RuntimeCode{{$}}
    // CHECK: [[CODE:RuntimeCodeTarget_runtime_code_[0-9]+]]: hex"
    // CHECK-LABEL: fn @runtime{{[( ]}}
    // CHECK: [[OBJECT:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[OBJECT]], [[#LEN:]]
    // CHECK: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[OBJECT]]
    // CHECK: [[TAIL:v[0-9]+]] = add [[DATA]], [[#mul(div(LEN, 32), 32)]]
    // CHECK: mstore [[TAIL]], 0
    // CHECK: data_copy [[CODE]], [[DATA]], [[#LEN]]
    // CHECK: [[RESULT:v[0-9]+]] = memory_object_len memorybytes, [[OBJECT]]
    // CHECK: ret [[RESULT]]
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
