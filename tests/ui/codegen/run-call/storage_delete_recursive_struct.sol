//@ run-call: clearNested() => 0, 0, 0, 0, 0, 0
//@ run-call: clearMutual() => 0, 0, 0, 0, 0, 0

contract StorageDeleteRecursiveStruct {
    struct Node {
        uint32 value;
        Node[] children;
    }

    struct A {
        uint32 value;
        B[] children;
    }

    struct B {
        uint32 value;
        A[] children;
    }

    Node private root;
    A private mutualRoot;

    function clearNested()
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        root.children.push();
        Node storage child = root.children[0];
        child.children.push();
        Node storage grandchild = child.children[0];
        assembly {
            sstore(root.slot, not(0))
            sstore(child.slot, not(0))
            sstore(grandchild.slot, not(0))
        }
        root.value = 1;
        child.value = 2;
        grandchild.value = 3;
        delete root;
        assembly {
            mstore(0, sload(root.slot))
            mstore(32, sload(add(root.slot, 1)))
            mstore(64, sload(child.slot))
            mstore(96, sload(add(child.slot, 1)))
            mstore(128, sload(grandchild.slot))
            mstore(160, sload(add(grandchild.slot, 1)))
            return(0, 192)
        }
    }

    function clearMutual()
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        B storage child = mutualRoot.children.push();
        A storage grandchild = child.children.push();
        assembly {
            sstore(mutualRoot.slot, not(0))
            sstore(child.slot, not(0))
            sstore(grandchild.slot, not(0))
        }
        mutualRoot.value = 1;
        child.value = 2;
        grandchild.value = 3;
        delete mutualRoot;
        assembly {
            mstore(0, sload(mutualRoot.slot))
            mstore(32, sload(add(mutualRoot.slot, 1)))
            mstore(64, sload(child.slot))
            mstore(96, sload(add(child.slot, 1)))
            mstore(128, sload(grandchild.slot))
            mstore(160, sload(add(grandchild.slot, 1)))
            return(0, 192)
        }
    }
}
