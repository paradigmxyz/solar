//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageCheckedArithmetic {
    struct Account {
        uint128 small;
        int8 signed;
    }

    mapping(address => uint256) balance;
    mapping(address => Account) accounts;

    // CHECK-LABEL: fn @storage_sub{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: [[OLD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: lt [[OLD]], arg1
    // CHECK: sstore {{v[0-9]+}}, [[NEW]]
    function storage_sub(address owner, uint256 amount) public {
        balance[owner] -= amount;
    }

    // CHECK-LABEL: fn @storage_binary_sub{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: [[OLD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: lt [[OLD]], arg1
    // CHECK: sstore {{v[0-9]+}}, [[NEW]]
    function storage_binary_sub(address owner, uint256 amount) public {
        balance[owner] = balance[owner] - amount;
    }

    // CHECK-LABEL: fn @storage_struct_add{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[OLD:v[0-9]+]] = and [[WORD]], 0xffffffffffffffffffffffffffffffff
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], arg1
    // CHECK: gt [[NEW]], 0xffffffffffffffffffffffffffffffff
    // CHECK: sload {{v[0-9]+}}
    // CHECK: not 0xffffffffffffffffffffffffffffffff
    // CHECK: and {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffff
    // CHECK: or {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sstore {{v[0-9]+}}, {{v[0-9]+}}
    function storage_struct_add(address owner, uint128 amount) public {
        accounts[owner].small += amount;
    }

    // CHECK-LABEL: fn @storage_struct_signed_sub{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[BASE]]
    // CHECK: [[SHIFTED:v[0-9]+]] = shr 128, [[WORD]]
    // CHECK: [[RAW:v[0-9]+]] = and [[SHIFTED]], 255
    // CHECK: [[OLD:v[0-9]+]] = signextend 0, [[RAW]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: slt [[NEW]], 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
    // CHECK: sgt [[NEW]], 127
    // CHECK: sload {{v[0-9]+}}
    // CHECK: not {{v[0-9]+}}
    // CHECK: and {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: and {{v[0-9]+}}, 255
    // CHECK: shl 128, {{v[0-9]+}}
    // CHECK: or {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sstore {{v[0-9]+}}, {{v[0-9]+}}
    function storage_struct_signed_sub(address owner, int8 amount) public {
        accounts[owner].signed -= amount;
    }
}
