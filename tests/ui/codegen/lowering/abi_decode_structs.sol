//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=ADS

// `abi.decode` into structs, struct arrays, and mixed tuples routes through
// the recursive materializer that also decodes struct parameters: static
// structs decode inline, dynamic structs and struct arrays follow their tail
// offsets, and nested aggregates become memory pointers. Verified
// behaviorally against solc, including a static sub-struct followed by
// dynamic fields (whose head sizing previously mis-offset later fields).

contract AbiDecodeStructs {
    struct Flat { uint256 a; address b; bool c; }
    struct Dyn { uint256 id; string name; uint256[] nums; }
    struct Nested { Flat flat; Dyn dyn; bytes tail; }

    // ADS-LABEL: fn @dFlat
    // Static struct: fields decode inline from the head, no tail offset.
    // ADS: mload
    function dFlat(bytes memory b) public pure returns (uint256, address, bool) {
        Flat memory f = abi.decode(b, (Flat));
        return (f.a, f.b, f.c);
    }

    // ADS-LABEL: fn @dNested
    // A static sub-struct followed by dynamic fields; the sub-struct occupies
    // its full head width so later field offsets are correct.
    // ADS: mcopy
    function dNested(bytes memory b) public pure returns (uint256, string memory, uint256) {
        Nested memory n = abi.decode(b, (Nested));
        return (n.flat.a, n.dyn.name, n.tail.length);
    }

    // ADS-LABEL: fn @dDynArr
    // A dynamic array of dynamic structs: elements rebuild one at a time into
    // a fresh memory array of pointers.
    // ADS: mcopy
    function dDynArr(bytes memory b) public pure returns (uint256 count) {
        Dyn[] memory ds = abi.decode(b, (Dyn[]));
        count = ds.length;
    }
}
