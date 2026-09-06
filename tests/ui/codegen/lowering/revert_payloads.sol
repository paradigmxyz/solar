//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RevertPayloads {
    // CHECK-LABEL: fn @assert_panic{{[( ]}}
    // CHECK: {{v[0-9]+}} = iszero arg0
    // CHECK: panic_if {{v[0-9]+}}, 0x1
    function assert_panic(bool ok) public pure {
        assert(ok);
    }

    // CHECK-LABEL: fn @require_message{{[( ]}}
    // CHECK: {{v[0-9]+}} = iszero arg0
    // CHECK: icall @revert_error{{.*}}
    // CHECK: invalid
    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    // CHECK-LABEL: fn @revert_message{{[( ]}}
    // CHECK: icall @revert_error{{.*}}
    // CHECK: invalid
    function revert_message() public pure {
        revert("bad");
    }

    // CHECK-LABEL: fn @revert_hex_message{{[( ]}}
    // CHECK: icall @revert_error{{.*}}
    // CHECK: invalid
    function revert_hex_message() public pure {
        revert(hex"626164");
    }
}
