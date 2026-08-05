//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract MappingBytesValues {
    mapping(uint256 => bytes) data;
    mapping(uint256 => mapping(uint256 => string)) nested;

    // CHECK-LABEL: fn @set{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: {{v[0-9]+}} = memory_object_len memorybytes
    // CHECK: sstore [[SLOT]],
    function set(uint256 key, bytes memory value) external {
        data[key] = value;
    }

    // CHECK-LABEL: fn @setNested{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot arg1, [[OUTER]]
    // CHECK: sstore [[INNER]],
    function setNested(uint256 outer, uint256 inner, string memory value) external {
        nested[outer][inner] = value;
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: ret [[SLOT]]
    function get(uint256 key) external view returns (bytes memory) {
        return data[key];
    }

    // CHECK-LABEL: fn @getNested{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[INNER:v[0-9]+]] = mapping_slot arg1, [[OUTER]]
    // CHECK: ret [[INNER]]
    function getNested(uint256 outer, uint256 inner) external view returns (string memory) {
        return nested[outer][inner];
    }
}
