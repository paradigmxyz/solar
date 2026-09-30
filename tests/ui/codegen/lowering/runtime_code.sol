//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RuntimeCodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract RuntimeCode {
    // CHECK-LABEL: fn @runtime{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = datasize RuntimeCodeTarget_runtime_code_0
    // CHECK: [[SIZE:v[0-9]+]] = datasize RuntimeCodeTarget_runtime_code_0, 63, aligned
    // CHECK: [[OBJECT:v[0-9]+]] = alloc memorybytes, {{.*}}, [[SIZE]]
    // CHECK: set_memory_object_len memorybytes, [[OBJECT]], [[LEN]]
    // CHECK: [[DATA_PTR:v[0-9]+]] = memory_object_data memorybytes, [[OBJECT]]
    // CHECK: [[TAIL:v[0-9]+]] = datasize RuntimeCodeTarget_runtime_code_0, 31, aligned
    // CHECK: [[BASE:v[0-9]+]] = ptrtoint memorybytes [[OBJECT]] to i256
    // CHECK: [[TAIL_PTR:v[0-9]+]] = add [[BASE]], [[TAIL]]
    // CHECK: mstore [[TAIL_PTR]], 0
    // CHECK: [[DATA:v[0-9]+]] = ptrtoint memptr [[DATA_PTR]] to i256
    // CHECK: datacopy RuntimeCodeTarget_runtime_code_0, [[DATA]], [[LEN]]
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
