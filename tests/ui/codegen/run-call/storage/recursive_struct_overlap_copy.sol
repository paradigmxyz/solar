//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: deep => 1, 1, 0
//@ run-call: arrayElement => 42, 42, 0
//@ run-call: arrayMember => 1, 1, 0
//@ run-call: bytesMember => 5, 1, 2
// ported-from: test/libsolidity/semanticTests/structs/copy_recursive_struct_overlap_target_contains_source_deep.sol
// ported-from: test/libsolidity/semanticTests/structs/copy_recursive_struct_overlap_target_contains_source_array_element.sol
// ported-from: test/libsolidity/semanticTests/structs/copy_recursive_struct_overlap_target_contains_source_array_member_first.sol
// ported-from: test/libsolidity/semanticTests/structs/copy_recursive_struct_overlap_target_contains_source_with_bytes_member.sol

// Overwriting a recursive struct with one of its own descendants leaves the descendant's value,
// not a mix of the old target and source.
contract RecursiveStructOverlapCopy {
    struct Node {
        Node[] children;
        uint256 balance;
    }

    struct DataNode {
        DataNode[] children;
        bytes data;
    }

    Node root;
    Node[] roots;
    DataNode dataRoot;

    // CHECK-LABEL: fn @deep()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function deep() external returns (uint256 sourceBalance, uint256 targetBalance, uint256 targetChildCount) {
        root.children.push();
        root.children[0].children.push();
        root.children[0].children[0].balance = 1;
        sourceBalance = root.children[0].children[0].balance;
        root = root.children[0].children[0];
        targetBalance = root.balance;
        targetChildCount = root.children.length;
    }

    // CHECK-LABEL: fn @arrayElement()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function arrayElement() external returns (uint256 sourceBalance, uint256 targetBalance, uint256 targetChildCount) {
        roots.push();
        roots[0].children.push();
        roots[0].children[0].balance = 42;
        sourceBalance = roots[0].children[0].balance;
        roots[0] = roots[0].children[0];
        targetBalance = roots[0].balance;
        targetChildCount = roots[0].children.length;
    }

    // CHECK-LABEL: fn @arrayMember()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function arrayMember() external returns (uint256 sourceBalance, uint256 targetBalance, uint256 targetChildCount) {
        root.children.push();
        root.children[0].balance = 1;
        sourceBalance = root.children[0].balance;
        root = root.children[0];
        targetBalance = root.balance;
        targetChildCount = root.children.length;
    }

    // CHECK-LABEL: fn @bytesMember()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function bytesMember() external returns (uint256 targetDataLength, uint256 targetChildCount, uint256 childDataLength) {
        dataRoot.children.push();
        dataRoot.children[0].data = "hello";
        dataRoot.children[0].children.push();
        dataRoot.children[0].children[0].data = "hi";
        dataRoot = dataRoot.children[0];
        targetDataLength = dataRoot.data.length;
        targetChildCount = dataRoot.children.length;
        childDataLength = dataRoot.children[0].data.length;
    }
}
