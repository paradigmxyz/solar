//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: assignIntoSelf => 7, 1, 5, 11, 7, 1
//@ run-call: pushSelf => 2, 7, 1

// solc 0.8.39 leaves a copy whose source contains its target undefined, and its tests expect the
// call to fail. We read the whole source before writing, so the target takes the source's old value.
contract RecursiveStructSourceContainsTarget {
    struct Node {
        Node[] children;
        uint256 balance;
    }

    Node root;

    function setUp() internal {
        root.balance = 7;
        root.children.push();
        root.children[0].balance = 5;
        root.children[0].children.push();
        root.children[0].children[0].balance = 11;
    }

    // CHECK-LABEL: fn @assignIntoSelf()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function assignIntoSelf()
        external
        returns (
            uint256 targetBalance,
            uint256 targetChildCount,
            uint256 targetChildBalance,
            uint256 targetGrandchildBalance,
            uint256 rootBalance,
            uint256 rootChildCount
        )
    {
        setUp();
        root.children[0].children[0] = root;
        Node storage target = root.children[0].children[0];
        targetBalance = target.balance;
        targetChildCount = target.children.length;
        targetChildBalance = target.children[0].balance;
        targetGrandchildBalance = target.children[0].children[0].balance;
        rootBalance = root.balance;
        rootChildCount = root.children.length;
    }

    // CHECK-LABEL: fn @pushSelf()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function pushSelf() external returns (uint256 childCount, uint256 pushedBalance, uint256 pushedChildCount) {
        setUp();
        root.children.push(root);
        childCount = root.children.length;
        pushedBalance = root.children[1].balance;
        pushedChildCount = root.children[1].children.length;
    }
}
