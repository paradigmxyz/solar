//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesElements {
    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: storage_array_data_slot 0
    // CHECK: ret {{v[0-9]+}}
    bytes public b;

    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: internal_call @store_storage_bytes, 0, 0, arg0
    function init(bytes memory value) public {
        b = value;
    }

    // CHECK-LABEL: fn @poke{{[( ]}}
    // CHECK: storage_array_data_slot 0
    // CHECK: memory_object_store_byte memorybytes
    // CHECK: internal_call @store_storage_bytes, 0, 0,
    function poke() public {
        b[5] = 0xAA;
    }

    // CHECK-LABEL: fn @hashB{{[( ]}}
    // CHECK: storage_array_data_slot 0
    // CHECK: keccak256_bytes {{v[0-9]+}}
    function hashB() public view returns (bytes32) {
        return keccak256(b);
    }
}

contract StorageStringConstructor {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: internal_call @store_storage_bytes, 0, 0, arg0
    // CHECK: internal_call @store_storage_bytes, 0, 1, arg1
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringBase {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: internal_call @store_storage_bytes, 0, 0, arg0
    // CHECK: internal_call @store_storage_bytes, 0, 1, arg1
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringDerived is StorageStringBase {
    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 9
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 4
    // CHECK: internal_call @store_storage_bytes, 0, 0,
    // CHECK: internal_call @store_storage_bytes, 0, 1,
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 0
    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: internal_call @load_storage_bytes, 1, 1
    constructor() StorageStringBase("ERC20Mock", "E20M") {}
}

// CHECK-LABEL: fn @constructor{{[( ]}}
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 17
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 3
// CHECK: internal_call @store_storage_bytes, 0, 0,
// CHECK: internal_call @store_storage_bytes, 0, 1,
contract StorageStringImplicitDerived is StorageStringBase("Base Literal Name", "BLN") {}
