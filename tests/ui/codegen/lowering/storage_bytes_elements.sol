//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract StorageBytesElements {
    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: sload 0
    // CHECK: ret
    bytes public b;

    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: {{v[0-9]+}} = memory_object_len memorybytes
    // CHECK: sstore 0,
    function init(bytes memory value) public {
        b = value;
    }

    // CHECK-LABEL: fn @poke{{[( ]}}
    // CHECK: sload 0
    // CHECK: memory_object_store_byte memorybytes
    // CHECK: stop
    function poke() public {
        b[5] = 0xAA;
    }

    // CHECK-LABEL: fn @hashB{{[( ]}}
    // CHECK: sload 0
    // CHECK: keccak256_bytes
    function hashB() public view returns (bytes32) {
        return keccak256(b);
    }
}

contract StorageStringConstructor {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @_anonymous{{.*abi_args=lazy.*}}
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_slice_load_word memory
    // CHECK: sstore 0,
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_slice_load_word memory
    // CHECK: sstore 1,
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringBase {
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: sstore 0,
    // CHECK: sstore 1,
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringDerived is StorageStringBase {
    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 9
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 4
    // CHECK: sstore 0,
    // CHECK: sstore 1,
    // CHECK-LABEL: fn @name{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 0
    // CHECK-LABEL: fn @symbol{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @__load_storage_bytes, 1, 1
    constructor() StorageStringBase("ERC20Mock", "E20M") {}
}

// CHECK-LABEL: fn @constructor{{[( ]}}
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 17
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 3
// CHECK: sstore 0,
// CHECK: sstore 1,
contract StorageStringImplicitDerived is StorageStringBase("Base Literal Name", "BLN") {}
