//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesElements {
    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: load_storage_bytes 0
    // CHECK: ret {{v[0-9]+}}
    bytes public b;

    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: icall @store_storage_bytes, 0, arg0
    function init(bytes memory value) public {
        b = value;
    }

    // CHECK-LABEL: fn @poke{{[( ]}}
    // CHECK: sload 0
    // CHECK: storage_array_data_slot 0
    // CHECK: [[WORD:v[0-9]+]] = phi
    // CHECK: sload [[WORD]]
    // CHECK: sstore [[WORD]]
    // CHECK-NOT: store_storage_bytes
    function poke() public {
        b[5] = 0xAA;
    }

    // CHECK-LABEL: fn @hashB{{[( ]}}
    // CHECK: load_storage_bytes 0
    // CHECK: keccak256_bytes {{v[0-9]+}}
    function hashB() public view returns (bytes32) {
        return keccak256(b);
    }
}

contract StorageStringConstructor {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: icall @load_storage_bytes, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: icall @load_storage_bytes, 1
    string public symbol;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: icall @store_storage_bytes, 0, arg0
    // CHECK: icall @store_storage_bytes, 1, arg1
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringBase {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: icall @load_storage_bytes, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: icall @load_storage_bytes, 1
    string public symbol;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: icall @store_storage_bytes, 0, arg0
    // CHECK: icall @store_storage_bytes, 1, arg1
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringDerived is StorageStringBase {
    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: [[STR:v[0-9]+]] = alloc memorybytes
    // CHECK-NEXT: [[LEN_PTR:v[0-9]+]] = ptrtoint memptr [[STR]] to i256
    // CHECK-NEXT: mstore [[LEN_PTR]], 9
    // CHECK: [[STR:v[0-9]+]] = alloc memorybytes
    // CHECK-NEXT: [[LEN_PTR:v[0-9]+]] = ptrtoint memptr [[STR]] to i256
    // CHECK-NEXT: mstore [[LEN_PTR]], 4
    // CHECK: icall @store_storage_bytes, 0,
    // CHECK: icall @store_storage_bytes, 1,
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: icall @load_storage_bytes, 0
    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: icall @load_storage_bytes, 1
    constructor() StorageStringBase("ERC20Mock", "E20M") {}
}

// CHECK-LABEL: fn @constructor{{[( ]}}
// CHECK: [[STR:v[0-9]+]] = alloc memorybytes
// CHECK-NEXT: [[LEN_PTR:v[0-9]+]] = ptrtoint memptr [[STR]] to i256
// CHECK-NEXT: mstore [[LEN_PTR]], 17
// CHECK: [[STR:v[0-9]+]] = alloc memorybytes
// CHECK-NEXT: [[LEN_PTR:v[0-9]+]] = ptrtoint memptr [[STR]] to i256
// CHECK-NEXT: mstore [[LEN_PTR]], 3
// CHECK: icall @store_storage_bytes, 0,
// CHECK: icall @store_storage_bytes, 1,
contract StorageStringImplicitDerived is StorageStringBase("Base Literal Name", "BLN") {}
