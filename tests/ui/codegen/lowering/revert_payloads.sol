//@compile-flags: -O none -Zdump=mir
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
    // CHECK: mstore 0, 0x8c379a0{{[0]+}}
    // CHECK: mstore 4, 32
    // CHECK: mstore 36, 3
    // CHECK: mstore 68, 0x626164{{[0]+}}
    // CHECK: revert 0, 100
    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    // CHECK-LABEL: fn @revert_message{{[( ]}}
    // CHECK: mstore 0, 0x8c379a0{{[0]+}}
    // CHECK: mstore 4, 32
    // CHECK: mstore 36, 3
    // CHECK: mstore 68, 0x626164{{[0]+}}
    // CHECK: revert 0, 100
    function revert_message() public pure {
        revert("bad");
    }

    // CHECK-LABEL: fn @revert_hex_message{{[( ]}}
    // CHECK: mstore 0, 0x8c379a0{{[0]+}}
    // CHECK: mstore 4, 32
    // CHECK: mstore 36, 3
    // CHECK: mstore 68, 0x626164{{[0]+}}
    // CHECK: revert 0, 100
    function revert_hex_message() public pure {
        revert(hex"626164");
    }
}
