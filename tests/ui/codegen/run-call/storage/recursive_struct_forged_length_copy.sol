//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: overlap() => 0x
//@ run-call-fail: disjoint() => 0x
// ported-from: test/libsolidity/semanticTests/structs/copy_recursive_struct_overlap_length_mul_overflow_reverts.sol

// A recursive copy whose target contains its source, after assembly forged the target array's
// length, runs out of gas in `overlap()` as it clears the old elements one by one. solc doubles the
// length in its overlap check and reverts with `Panic(0x11)`. A disjoint copy runs out of gas in
// both compilers. See CODEGEN-031 in docs/SOLC_DIVERGENCE.md.
contract C {
    struct Node {
        Node[] children;
        uint256 v;
    }

    Node root;
    Node other;

    // CHECK-LABEL: fn @overlap()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function overlap() external {
        root.children.push();
        root.children[0].v = 9;
        assembly {
            sstore(root.slot, shl(255, 1))
        }
        root = root.children[0];
    }

    // CHECK-LABEL: fn @disjoint()
    // CHECK: icall @load_storage_struct_array_
    // CHECK: icall @store_recursive_storage_
    function disjoint() external {
        root.children.push();
        root.children[0].v = 9;
        assembly {
            sstore(other.slot, shl(255, 1))
        }
        other = root.children[0];
    }
}
