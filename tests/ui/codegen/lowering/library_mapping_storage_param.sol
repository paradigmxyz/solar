//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// A `library` function may take a `mapping`/`storage` reference parameter and
// bind a storage-reference local from it:
//
//     function sum(mapping(address => Reserve) storage m, address k) internal {
//         Reserve storage r = m[k];   // r is a storage reference, not a copy
//         return r.a + r.b;           // read via SLOAD
//     }
//
// The caller passes the mapping's base slot by value; the callee resolves the
// element slot with keccak256 and reads the struct fields with `sload`. This
// used to either ICE (mapping parameter not resolvable to a slot) or silently
// miscompile (the storage-reference local read as an in-memory struct via
// `mload`, and the base slot `sload`'d instead of passed as a constant). The
// runtime result is verified against solc 0.8.30 separately; this test pins the
// compiled bytecode so the two failure modes cannot regress unnoticed.

struct Reserve {
    uint256 a;
    uint256 b;
}

library L {
    function sum(mapping(address => Reserve) storage m, address k)
        internal
        view
        returns (uint256)
    {
        Reserve storage r = m[k];
        return r.a + r.b;
    }
}

contract C {
    mapping(address => Reserve) reserves;

    function total(address k) external view returns (uint256) {
        return L.sum(reserves, k);
    }
}
