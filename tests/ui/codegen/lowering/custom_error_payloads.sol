//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CustomErrorPayloads {
    error EmptyError();
    error MyError(uint256 code, string message);

    // CHECK-LABEL: fn @revert_empty{{[( ]}}
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [], selector 0x{{[0-9a-f]+}}
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function revert_empty() public pure {
        revert EmptyError();
    }

    // CHECK-LABEL: fn @revert_args{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[MESSAGE]], 6
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [word, memory_bytes], selector 0x{{[0-9a-f]+}}, args 7, [[MESSAGE]]
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function revert_args() public pure {
        revert MyError(7, "failed");
    }

    // CHECK-LABEL: fn @require_empty{{[( ]}}
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: jumpi [[FAIL]],
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [], selector 0x{{[0-9a-f]+}}
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function require_empty(bool ok) public pure {
        require(ok, EmptyError());
    }

    // CHECK-LABEL: fn @require_named{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: jumpi [[FAIL]],
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [word, memory_bytes], selector 0x{{[0-9a-f]+}}, args 7, [[MESSAGE]]
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function require_named(bool ok) public pure {
        require(ok, MyError({message: "failed", code: 7}));
    }
}
