//@ codegen-matrix: standard
//@ run-call: 0x081236370000000000000000000000000000000000000000000000000000000000000000 => 0x000000000000000000000000000000000000000000000000000000000000002a
//@ run-call: 0xec7fa3c70000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000

// Recursive library storage pointers have selectors and dispatch routes even
// though these functions are omitted from the JSON ABI. Call them as raw calldata.
library Recursive {
    struct Node { uint value; Node[] children; }
    struct MapNode { uint value; mapping(uint => MapNode) children; }

    function read(Node storage node) public view returns (uint) {
        return node.value + 42;
    }

    function count(Node storage node) external view returns (uint) {
        return node.children.length;
    }

    function child(Node storage node, uint index) external view returns (Node storage) {
        return node.children[index];
    }

    function arrayCount(Node[] storage nodes) public view returns (uint) {
        return nodes.length;
    }

    function mapped(MapNode storage node, uint key) external view returns (uint) {
        return node.children[key].value;
    }

    function mapRef(mapping(uint => MapNode) storage nodes) public pure
        returns (mapping(uint => MapNode) storage)
    {
        return nodes;
    }
}
