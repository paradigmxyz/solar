//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: 0x081236370000000000000000000000000000000000000000000000000000000000000000 => 0x000000000000000000000000000000000000000000000000000000000000002a
//@ run-call: 0xec7fa3c70000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000

// Recursive library storage pointers have selectors and dispatch routes even
// though these functions are omitted from the JSON ABI. Call them as raw calldata.
library Recursive {
    struct Node { uint value; Node[] children; }
    struct MapNode { uint value; mapping(uint => MapNode) children; }

    // CHECK-LABEL: fn @read(
    // CHECK-SAME: selector=0x08123637
    // CHECK-SAME: abi_params=[storageptr]
    // CHECK: sload arg0
    // CHECK: checked_add {{[^,]+}}, {{v[0-9]+}}, 42
    // CHECK: ret
    function read(Node storage node) public view returns (uint) {
        return node.value + 42;
    }

    // CHECK-LABEL: fn @count(
    // CHECK-SAME: selector=0xec7fa3c7
    // CHECK-SAME: abi_params=[storageptr]
    // CHECK: [[LENGTH_SLOT:v[0-9]+]] = add arg0, 1
    // CHECK: sload [[LENGTH_SLOT]]
    // CHECK: ret
    function count(Node storage node) external view returns (uint) {
        return node.children.length;
    }

    // CHECK-LABEL: fn @child(
    // CHECK-SAME: selector=0xfdf833f5
    // CHECK-SAME: abi_params=[storageptr, u256]
    // CHECK: storage_array_data_slot
    // CHECK: ret
    function child(Node storage node, uint index) external view returns (Node storage) {
        return node.children[index];
    }

    // CHECK-LABEL: fn @arrayCount(
    // CHECK-SAME: selector=0x9c7ebd74
    // CHECK-SAME: abi_params=[storageptr]
    // CHECK: sload arg0
    // CHECK: ret
    function arrayCount(Node[] storage nodes) public view returns (uint) {
        return nodes.length;
    }

    // CHECK-LABEL: fn @mapped(
    // CHECK-SAME: selector=0x6a49b46f
    // CHECK-SAME: abi_params=[storageptr, u256]
    // CHECK: [[MAPPED_SLOT:v[0-9]+]] = mapping_slot arg1,
    // CHECK: sload [[MAPPED_SLOT]]
    // CHECK: ret
    function mapped(MapNode storage node, uint key) external view returns (uint) {
        return node.children[key].value;
    }

    // CHECK-LABEL: fn @mapRef(
    // CHECK-SAME: selector=0x47b296b8
    // CHECK-SAME: abi_params=[storageptr]
    // CHECK: ret arg0
    function mapRef(mapping(uint => MapNode) storage nodes) public pure
        returns (mapping(uint => MapNode) storage)
    {
        return nodes;
    }
}
