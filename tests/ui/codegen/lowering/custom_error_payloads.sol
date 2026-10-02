//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CustomErrorPayloads {
    error EmptyError();
    error MyError(uint256 code, string message);

    // CHECK-LABEL: fn @revert_empty{{[( ]}}
    // CHECK: icall require<custom_error []>, false, 0x{{[0-9a-f]+}}
    // CHECK: invalid
    function revert_empty() public pure {
        revert EmptyError();
    }

    // CHECK-LABEL: fn @revert_args{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[MESSAGE]], 6
    // CHECK: icall require<custom_error [word, memory_bytes]>, false, 0x{{[0-9a-f]+}}, 7, [[MESSAGE]]
    // CHECK: invalid
    function revert_args() public pure {
        revert MyError(7, "failed");
    }

    // CHECK-LABEL: fn @require_empty{{[( ]}}
    // CHECK: [[COND:v[0-9]+]] = ne arg0, 0
    // CHECK: icall require<custom_error []>, [[COND]], 0x{{[0-9a-f]+}}
    function require_empty(bool ok) public pure {
        require(ok, EmptyError());
    }

    // CHECK-LABEL: fn @require_named{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: [[COND:v[0-9]+]] = ne arg0, 0
    // CHECK: icall require<custom_error [word, memory_bytes]>, [[COND]], 0x{{[0-9a-f]+}}, 7, [[MESSAGE]]
    function require_named(bool ok) public pure {
        require(ok, MyError({message: "failed", code: 7}));
    }
}
