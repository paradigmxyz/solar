//@compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-arithmetic
//@filecheck:

contract StorageCheckedArithmetic {
    struct Account {
        uint128 small;
        int8 signed;
    }

    mapping(address => uint256) balance;
    mapping(address => Account) accounts;

    // CHECK-LABEL: fn @storage_sub{{[( ]}}
    // CHECK: [[KEY:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot [[KEY]], 0
    // CHECK: [[OLD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: lt [[OLD]], arg1
    // CHECK: sstore {{v[0-9]+}}, [[NEW]]
    function storage_sub(address owner, uint256 amount) public {
        balance[owner] -= amount;
    }

    // CHECK-LABEL: fn @storage_binary_sub{{[( ]}}
    // CHECK: [[KEY:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot [[KEY]], 0
    // CHECK: [[OLD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: lt [[OLD]], arg1
    // CHECK: sstore {{v[0-9]+}}, [[NEW]]
    function storage_binary_sub(address owner, uint256 amount) public {
        balance[owner] = balance[owner] - amount;
    }

    // CHECK-LABEL: fn @storage_struct_add{{[( ]}}
    // CHECK: [[KEY:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot [[KEY]], 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[NARROW:v[0-9]+]] = trunc i256 [[WORD]] to i128
    // CHECK: [[LHS:v[0-9]+]] = zext i128 [[NARROW]] to i256
    // CHECK: [[AMOUNT:v[0-9]+]] = zext i128 arg1 to i256
    // CHECK: [[NEW:v[0-9]+]] = add [[LHS]], [[AMOUNT]]
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
    // CHECK: [[KEY:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: [[BASE:v[0-9]+]] = mapping_slot [[KEY]], 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[BASE]]
    // CHECK: [[SHIFTED:v[0-9]+]] = shr 128, [[WORD]]
    // CHECK: [[OLD:v[0-9]+]] = signextend 0, [[SHIFTED]]
    // CHECK: [[NARROW:v[0-9]+]] = trunc i256 [[OLD]] to i8
    // CHECK: [[LHS:v[0-9]+]] = sext i8 [[NARROW]] to i256
    // CHECK: [[AMOUNT:v[0-9]+]] = sext i8 arg1 to i256
    // CHECK: [[NEW:v[0-9]+]] = sub [[LHS]], [[AMOUNT]]
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
