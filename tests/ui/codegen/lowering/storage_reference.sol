//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// Storage references are modeled as slot values: `Item storage r = items[k]`
// binds the storage *slot*, so `r.a` reads/writes `sload`/`sstore(slot + off)`
// rather than dereferencing the loaded value as a memory pointer. Indexed field
// access (`items[k].a`) shares the same slot computation. Previously both forms
// were miscompiled as memory accesses. Runtime-verified against solc.
contract C {
    struct Item {
        uint256 a;
        uint256 b;
    }

    mapping(uint256 => Item) items;

    // Direct indexed storage struct field assignment.
    function setDirect(uint256 k, uint256 a) public {
        items[k].a = a;
    }

    // Read through a storage reference: `r` holds the slot of `items[k]`.
    function getViaRef(uint256 k) public view returns (uint256) {
        Item storage r = items[k];
        return r.a;
    }

    // Read/modify/write through a storage reference.
    function bump(uint256 k) public {
        Item storage r = items[k];
        r.b = r.b + 1;
    }
}
