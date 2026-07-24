//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract StorageBytesElements {
    // CHECK-LABEL: fn @b
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    bytes public b;

    // CHECK-LABEL: fn @init
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: sload 0
    // CHECK: sstore 0,
    function init(bytes memory value) public {
        b = value;
    }

    // CHECK-LABEL: fn @poke
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[LOW_BIT:v[0-9]+]] = and [[WORD]], 1
    // CHECK: [[LONG:v[0-9]+]] = eq [[LOW_BIT]], 1
    // CHECK: br {{v[0-9]+}},
    // CHECK: br [[LONG]],
    // CHECK: sstore 0,
    // CHECK: [[DATA:v[0-9]+]] = keccak256 0, 32
    // CHECK: sstore {{v[0-9]+}},
    function poke() public {
        b[5] = 0xAA;
    }

    // CHECK-LABEL: fn @hashB
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: keccak256_bytes [[VALUE]]
    function hashB() public view returns (bytes32) {
        return keccak256(b);
    }
}

contract StorageStringConstructor {
    // CHECK-LABEL: fn @name
    // CHECK: internal_call @__load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol
    // CHECK: internal_call @__load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @_anonymous
    // CHECK-COUNT-2: set_memory_object_len memorybytes
    // CHECK: sstore 0,
    // CHECK: sstore 1,
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringBase {
    // CHECK-LABEL: fn @name
    // CHECK: internal_call @__load_storage_bytes, 1, 0
    string public name;

    // CHECK-LABEL: fn @symbol
    // CHECK: internal_call @__load_storage_bytes, 1, 1
    string public symbol;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: sstore 0,
    // CHECK: sstore 1,
    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringDerived is StorageStringBase {
    // CHECK-LABEL: fn @_anonymous
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 9
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 4
    // CHECK: sstore 0,
    // CHECK: sstore 1,
    // CHECK-LABEL: fn @name
    // CHECK: internal_call @__load_storage_bytes, 1, 0
    // CHECK-LABEL: fn @symbol
    // CHECK: internal_call @__load_storage_bytes, 1, 1
    constructor() StorageStringBase("ERC20Mock", "E20M") {}
}

// CHECK-LABEL: fn @constructor
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 17
// CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 3
// CHECK: sstore 0,
// CHECK: sstore 1,
// CHECK-LABEL: fn @name
// CHECK: internal_call @__load_storage_bytes, 1, 0
// CHECK-LABEL: fn @symbol
// CHECK: internal_call @__load_storage_bytes, 1, 1
contract StorageStringImplicitDerived is StorageStringBase("Base Literal Name", "BLN") {}
