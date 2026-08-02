//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract RevertPayloads {
    // CHECK-LABEL: fn @assert_panic{{[( ]}}
    // CHECK: {{v[0-9]+}} = iszero arg0
    // CHECK: mstore 0, 0x4e487b71{{[0]+}}
    // CHECK: mstore 4, 1
    // CHECK: revert 0, 36
    function assert_panic(bool ok) public pure {
        assert(ok);
    }

    // CHECK-LABEL: fn @require_message{{[( ]}}
    // CHECK: {{v[0-9]+}} = iszero arg0
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[MESSAGE]], 3
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [memory_bytes], selector 0x8c379a0{{[0]+}}, args [[MESSAGE]]
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    // CHECK-LABEL: fn @revert_message{{[( ]}}
    // CHECK: [[MESSAGE:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[MESSAGE]], 3
    // CHECK: [[PAYLOAD:v[0-9]+]] = abi_encode [memory_bytes], selector 0x8c379a0{{[0]+}}, args [[MESSAGE]]
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[PAYLOAD]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[PAYLOAD]]
    // CHECK: revert [[PTR]], [[LEN]]
    function revert_message() public pure {
        revert("bad");
    }
}
