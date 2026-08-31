//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract MappingDynamicKey {
    // CHECK-LABEL: fn @lookup{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_calldata arg0, 0
    // CHECK: sload [[SLOT]]
    mapping(string => address) public lookup;

    // CHECK-LABEL: fn @set{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_memory arg0, 0
    // CHECK: sstore [[SLOT]], {{v[0-9]+}}
    function set(string memory name, address owner) public {
        lookup[name] = owner;
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_memory arg0, 0
    // CHECK: sload [[SLOT]]
    function get(string memory name) public view returns (address) {
        return lookup[name];
    }
}

// Every dynamic-key path must hash `key bytes ++ 32-byte slot` per spec
// (applied per level for nested mappings).
contract MappingDynamicKeyPaths {
    // CHECK-LABEL: fn @flat{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_calldata arg0, 0
    // CHECK: sload [[SLOT]]
    mapping(string => uint256) public flat;

    // CHECK-LABEL: fn @nestedFirst{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot_calldata arg0, 1
    // CHECK: [[KEY:v[0-9]+]] = and arg1, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot [[KEY]], [[OUTER]]
    mapping(string => mapping(address => uint256)) public nestedFirst;

    // CHECK-LABEL: fn @nestedSecond{{[( ]}}
    // CHECK: [[KEY:v[0-9]+]] = and arg0, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot [[KEY]], 2
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot_calldata arg1, [[OUTER]]
    mapping(address => mapping(string => uint256)) public nestedSecond;

    // CHECK-LABEL: fn @skey{{[( ]}}
    // CHECK: storage_array_data_slot 3
    // CHECK: ret {{v[0-9]+}}
    string public skey;

    // Literal keys hash exactly the literal's bytes, hitting the same slot
    // as the equivalent runtime key.
    // CHECK-LABEL: fn @setLit{{[( ]}}
    // CHECK: [[LITERAL_DATA:v[0-9]+]] = memory_object_data memorybytes, {{v[0-9]+}}
    // CHECK: mstore [[LITERAL_DATA]], 0x68656c6c6f000000000000000000000000000000000000000000000000000000
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_memory {{v[0-9]+}}, 0
    // CHECK: sstore [[SLOT]], arg0
    function setLit(uint256 v) public {
        flat["hello"] = v;
    }

    // CHECK-LABEL: fn @setLitLong{{[( ]}}
    // CHECK: [[SLOT_LONG:v[0-9]+]] = mapping_slot_memory {{v[0-9]+}}, 0
    // CHECK: sstore [[SLOT_LONG]], arg0
    function setLitLong(uint256 v) public {
        flat["a literal key longer than thirty-two bytes, hashed in full"] = v;
    }

    // Nested mappings dispatch on the key type at every level.
    // CHECK-LABEL: fn @setNestedFirst{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot_memory arg0, 1
    // CHECK: [[KEY:v[0-9]+]] = and arg1, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot [[KEY]], [[OUTER]]
    // CHECK: sstore [[INNER]], arg2
    function setNestedFirst(string memory k, address a, uint256 v) public {
        nestedFirst[k][a] = v;
    }

    // CHECK-LABEL: fn @getNestedFirst{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot_memory arg0, 1
    // CHECK: [[KEY:v[0-9]+]] = and arg1, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot [[KEY]], [[OUTER]]
    // CHECK: sload [[INNER]]
    function getNestedFirst(string memory k, address a) public view returns (uint256) {
        return nestedFirst[k][a];
    }

    // CHECK-LABEL: fn @setNestedSecond{{[( ]}}
    // CHECK: [[KEY:v[0-9]+]] = and arg0, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot [[KEY]], 2
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot_memory arg1, [[OUTER]]
    // CHECK: sstore [[INNER]], arg2
    function setNestedSecond(address a, string memory k, uint256 v) public {
        nestedSecond[a][k] = v;
    }

    // Storage string key: materialized to memory, then hashed as bytes.
    // CHECK-LABEL: fn @setSkey{{[( ]}}
    // CHECK: internal_call @store_storage_bytes, 0, 3, arg0
    function setSkey(string memory s) public {
        skey = s;
    }

    // CHECK-LABEL: fn @setViaStorageKey{{[( ]}}
    // CHECK: storage_array_data_slot 3
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_memory {{v[0-9]+}}, 0
    // CHECK: sstore [[SLOT]], arg0
    function setViaStorageKey(uint256 v) public {
        flat[skey] = v;
    }

    // Calldata keys remain semantic until the mapping-slot lowering pass;
    // subsequent allocations must not overlap their payload.
    // CHECK-LABEL: fn @setThenAlloc{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot_calldata arg0, 0
    // CHECK: sstore [[SLOT]], arg1
    // CHECK: [[OUT:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic,
    // CHECK: set_memory_object_len memorybytes, [[OUT]], 32
    function setThenAlloc(string calldata k, uint256 v) public returns (uint256) {
        flat[k] = v;
        bytes memory out = new bytes(32);
        return out.length;
    }
}
