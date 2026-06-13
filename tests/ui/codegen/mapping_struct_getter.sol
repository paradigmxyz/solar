//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

// The auto-generated getter for a `mapping(K => Struct) public` must read the
// struct fields from the element's storage slot `keccak256(key, slot)`, not from
// an uninitialized temporary. The synthesized body is
// `Struct storage tmp = items[k]; return (tmp.a, tmp.b, tmp.c);` - previously
// `tmp` had no initializer, so the getter returned the wrong slot (slot 0).
// Runtime-verified against solc.
contract C {
    uint256 internal counter = 1; // slot 0

    struct Item {
        address a;
        uint256 b;
        address c;
    }

    mapping(uint256 => Item) public items; // slot 1
}
