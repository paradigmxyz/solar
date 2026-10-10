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
    // CHECK: [[HEAD:v[0-9]+]] = ptrtoint memptr [[OBJECT]] to i256
    // CHECK-NEXT: mstore [[HEAD]], [[LEN]]
    // CHECK: [[DATA_BASE:v[0-9]+]] = ptrtoint memptr [[OBJECT]] to i256
    // CHECK-NEXT: [[DATA:v[0-9]+]] = add [[DATA_BASE]], 32
    // CHECK: [[TAIL:v[0-9]+]] = datasize RuntimeCodeTarget_runtime_code_0, 31, aligned
    // CHECK: [[BASE:v[0-9]+]] = ptrtoint memptr [[OBJECT]] to i256
    // CHECK: [[TAIL_PTR:v[0-9]+]] = add [[BASE]], [[TAIL]]
    // CHECK: mstore [[TAIL_PTR]], 0
    // CHECK: datacopy RuntimeCodeTarget_runtime_code_0, [[DATA]], [[LEN]]
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
