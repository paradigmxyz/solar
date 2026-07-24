//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract CustomErrorPayloads {
    error EmptyError();
    error MyError(uint256 code, string message);

    // CHECK-LABEL: fn @revert_empty{{[( ]}}
    // CHECK: [[PAYLOAD:v[0-9]+]] = alloc raw
    // CHECK: mstore [[PAYLOAD]], 0x{{[0-9a-f]+}}
    // CHECK: revert [[PAYLOAD]], {{v[0-9]+}}
    function revert_empty() public pure {
        revert EmptyError();
    }

    // CHECK-LABEL: fn @revert_args{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[MESSAGE]], 6
    // CHECK: [[PAYLOAD:v[0-9]+]] = alloc raw
    // CHECK: mcopy
    // CHECK: revert [[PAYLOAD]],
    function revert_args() public pure {
        revert MyError(7, "failed");
    }

    // CHECK-LABEL: fn @require_empty{{[( ]}}
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: jumpi [[FAIL]],
    // CHECK: [[PAYLOAD:v[0-9]+]] = alloc raw
    // CHECK: revert [[PAYLOAD]],
    function require_empty(bool ok) public pure {
        require(ok, EmptyError());
    }

    // CHECK-LABEL: fn @require_named{{[( ]}}
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: jumpi [[FAIL]],
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: [[PAYLOAD:v[0-9]+]] = alloc raw
    // CHECK: revert [[PAYLOAD]],
    function require_named(bool ok) public pure {
        require(ok, MyError({message: "failed", code: 7}));
    }
}
