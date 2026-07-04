//@compile-flags: --emit=hashes

// A `library` (unlike a contract) may expose `public`/`external` functions
// that take `storage` reference parameters and refer to structs by name. solc
// encodes their signatures — and hence 4-byte selectors — with the struct's
// canonical name and a trailing `storage` location suffix: `total(S storage)`,
// not a flattened `total((uint256,uint256))`.
//
// Contract function signatures are unaffected: structs still flatten into ABI
// tuples and carry no location suffix.
//
// `get` is dropped from the interface (and hence from `hashes`) because its
// mapping parameter is not considered exportable yet (see the `interfaceType`
// TODO in `interface_functions`); solc lists it as
// `2aed1630: get(mapping(address => S) storage,address)`, which this printer
// already produces.

struct S {
    uint256 a;
    uint256 b;
}

library L {
    function get(mapping(address => S) storage m, address k) public view returns (uint256) {
        return m[k].a;
    }

    // keccak256("total(S storage)")[..4] == 0x33ad6f28, matching solc.
    function total(S storage s) external view returns (uint256) {
        return s.a + s.b;
    }
}

contract C {
    // keccak256("sum((uint256,uint256))")[..4] == 0x5601ffbe, matching solc.
    function sum(S memory s) external pure returns (uint256) {
        return s.a + s.b;
    }
}
