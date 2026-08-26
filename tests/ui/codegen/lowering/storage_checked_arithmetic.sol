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

    // `small` shares slot 0 of the packed struct: masked read, checked add,
    // then a read-modify-write that preserves the slot's other bytes.
    // CHECK-LABEL: fn @storage_struct_add{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[OLD:v[0-9]+]] = and [[WORD]], 0xffffffffffffffffffffffffffffffff
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], arg1
    // CHECK: gt [[NEW]], 0xffffffffffffffffffffffffffffffff
    // CHECK: [[KEEP:v[0-9]+]] = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffff00000000000000000000000000000000
    // CHECK: [[INS:v[0-9]+]] = and [[NEW]], 0xffffffffffffffffffffffffffffffff
    // CHECK: [[MERGED:v[0-9]+]] = or [[KEEP]], [[INS]]
    // CHECK: sstore {{v[0-9]+}}, [[MERGED]]
    function storage_struct_add(address owner, uint128 amount) public {
        accounts[owner].small += amount;
    }

    // `signed` packs at byte offset 16 of the same slot: shift-and-signextend
    // read, then a read-modify-write masking only its byte.
    // CHECK-LABEL: fn @storage_struct_signed_sub{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[WORD:v[0-9]+]] = sload [[BASE]]
    // CHECK: [[SHIFTED:v[0-9]+]] = shr 128, [[WORD]]
    // CHECK: [[OLD:v[0-9]+]] = signextend 0, [[SHIFTED]]
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg1
    // CHECK: slt [[NEW]], 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
    // CHECK: sgt [[NEW]], 127
    // CHECK: [[KEEP:v[0-9]+]] = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffff00ffffffffffffffffffffffffffffffff
    // CHECK: [[INS:v[0-9]+]] = and [[NEW]], 255
    // CHECK: [[UP:v[0-9]+]] = shl 128, [[INS]]
    // CHECK: [[MERGED:v[0-9]+]] = or [[KEEP]], [[UP]]
    // CHECK: sstore {{v[0-9]+}}, [[MERGED]]
    function storage_struct_signed_sub(address owner, int8 amount) public {
        accounts[owner].signed -= amount;
    }
}
