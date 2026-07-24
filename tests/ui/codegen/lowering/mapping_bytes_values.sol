//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract MappingBytesValues {
    mapping(uint256 => bytes) data;
    mapping(uint256 => mapping(uint256 => string)) nested;

    // CHECK-LABEL: fn @set
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: {{v[0-9]+}} = memory_object_len memorybytes
    // CHECK: sload [[SLOT]]
    // CHECK: sstore [[SLOT]],
    function set(uint256 key, bytes memory value) external {
        data[key] = value;
    }

    // CHECK-LABEL: fn @setNested
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot arg1, [[OUTER]]
    // CHECK: sload [[INNER]]
    // CHECK: sstore [[INNER]],
    function setNested(uint256 outer, uint256 inner, string memory value) external {
        nested[outer][inner] = value;
    }

    // CHECK-LABEL: fn @get
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, [[SLOT]]
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function get(uint256 key) external view returns (bytes memory) {
        return data[key];
    }

    // CHECK-LABEL: fn @getNested
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot arg1, [[OUTER]]
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, [[INNER]]
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function getNested(uint256 outer, uint256 inner) external view returns (string memory) {
        return nested[outer][inner];
    }
}
