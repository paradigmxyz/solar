//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// The auto-generated getter for a `mapping(K => Struct) public` must read the
// struct fields from the element's storage slot `keccak256(key, slot)`, not from
// an uninitialized temporary. The synthesized body is
// `Struct storage tmp = items[k]; return (tmp.a, tmp.b, tmp.c);` - previously
// `tmp` had no initializer, so the getter returned the wrong slot (slot 0).
// Runtime-verified against solc.
contract C {
    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: sstore 0, 1
    uint256 internal counter = 1; // slot 0

    struct Item {
        address a;
        uint256 b;
        address c;
    }

    // CHECK-LABEL: fn @items{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: sload [[BASE]]
    // CHECK: sload {{v[0-9]+}} !metadata(storage=offset([[BASE]], 1))
    // CHECK: sload {{v[0-9]+}} !metadata(storage=offset([[BASE]], 2))
    // CHECK: returndata 128, 96
    mapping(uint256 => Item) public items; // slot 1
}
