//@ revisions: hashes abi
//@[hashes] compile-flags: --emit=hashes
//@[abi] compile-flags: --emit=abi

// Library storage pointers are exported in method identifiers, but not JSON ABI.
// This fixture covers signatures independently of executable library wrappers.
library Recursive {
    struct Node { Node[] children; }
    struct MapNode { mapping(uint => MapNode) children; }

    function read(Node storage) public {}
    function count(Node storage) external {}
    function child(Node storage node) external pure returns (Node storage) { return node; }
    function arrayCount(Node[] storage) public {}
    function mapped(MapNode storage) external {}
    function mapRef(mapping(uint => MapNode) storage nodes) public pure
        returns (mapping(uint => MapNode) storage)
    {
        return nodes;
    }
}
