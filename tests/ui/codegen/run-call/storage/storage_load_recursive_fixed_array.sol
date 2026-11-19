//@ filecheck:
// CHECK: fn @load_storage_struct_array_0_2(
//@ codegen-matrix: standard
//@ run-call: partialAssignment => 1, 0, 0
//@ run-call: copy => 5, 6, 8, 6, 9, 0

contract StorageLoadRecursiveFixedArray {
    struct Node {
        Node[2][] pairs;
        uint256 v;
    }

    Node[3] t;
    Node a;
    Node b;

    function partialAssignment() external returns (uint256, uint256, uint256) {
        t[0].pairs.push();
        t[0].pairs[0][0].v = 1;
        t[1].v = 7;
        t[2].v = 9;

        t = t[0].pairs[0];

        return (t[0].v, t[1].v, t[2].v);
    }

    function copy() external returns (uint256, uint256, uint256, uint256, uint256, uint256) {
        b.v = 5;
        b.pairs.push();
        b.pairs[0][1].v = 6;
        b.pairs[0][1].pairs.push();
        b.pairs[0][1].pairs[0][0].v = 8;
        a.v = 1;

        a = b;
        Node memory m = a;
        b.pairs[0][1].v = 9;
        a.pairs[0][1].pairs.pop();

        return (
            a.v,
            m.pairs[0][1].v,
            m.pairs[0][1].pairs[0][0].v,
            a.pairs[0][1].v,
            b.pairs[0][1].v,
            a.pairs[0][1].pairs.length
        );
    }
}
