//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract RevertPayloads {
    // CHECK-LABEL: fn @assert_panic{{[( ]}}
    // CHECK: {{v[0-9]+}} = eq arg0, {{(0|false)}}
    // CHECK: icall panic_if<0x1>, {{v[0-9]+}}
    function assert_panic(bool ok) public pure {
        assert(ok);
    }

    // CHECK-LABEL: fn @require_message{{[( ]}}
    // CHECK: [[COND:v[0-9]+]] = ne arg0, 0
    // CHECK: icall require<short_string>, [[COND]], 3, 0x626164{{0+}}
    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    // CHECK-LABEL: fn @revert_message{{[( ]}}
    // CHECK: icall require<short_string>, false, 3, 0x626164{{0+}}
    // CHECK: invalid
    function revert_message() public pure {
        revert("bad");
    }

    // CHECK-LABEL: fn @revert_hex_message{{[( ]}}
    // CHECK: icall require<short_string>, false, 3, 0x626164{{0+}}
    // CHECK: invalid
    function revert_hex_message() public pure {
        revert(hex"626164");
    }
}
