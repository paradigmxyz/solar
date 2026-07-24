//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract RevertPayloads {
    // CHECK-LABEL: fn @assert_panic
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: mstore 0, 0x4e487b71{{[0]+}}
    // CHECK: mstore 4, 1
    // CHECK: revert 0, 36
    function assert_panic(bool ok) public pure {
        assert(ok);
    }

    // CHECK-LABEL: fn @require_message
    // CHECK: [[FAIL:v[0-9]+]] = iszero arg0
    // CHECK: internal_call @__revert_error, 0, 3, 0x626164{{[0]+}}
    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    // CHECK-LABEL: fn @revert_message
    // CHECK: internal_call @__revert_error, 0, 3, 0x626164{{[0]+}}
    function revert_message() public pure {
        revert("bad");
    }
}
