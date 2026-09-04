//@ compile-flags: --emit=abi,hashes --pretty-json

// A `library` may expose `public`/`external` functions taking or returning
// `storage` references. Storage references have no ABI encoding, so solc
// omits those functions from the JSON ABI, but still lists them in the
// method identifiers with a `storage` location suffix.

library L {
    struct Set {
        mapping(uint256 => uint256) idx;
    }

    struct Plain {
        uint256 a;
    }

    struct Nested {
        Plain p;
        Set s;
    }

    function withMapping(Set storage s) external view returns (uint256) {
        return s.idx[0];
    }

    function plainStruct(Plain storage p) external view returns (uint256) {
        return p.a;
    }

    function mappingParam(mapping(uint256 => uint256) storage m) external view returns (uint256) {
        return m[0];
    }

    function arrParam(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }

    function setArrParam(Set[] storage a) external view returns (uint256) {
        return a.length;
    }

    function nestedParam(Nested storage n) external view returns (uint256) {
        return n.p.a;
    }

    // `public`, not `external`.
    function pubParam(Set storage s) public view returns (uint256) {
        return s.idx[0];
    }

    // A `storage` return is omitted just like a `storage` parameter.
    function storageReturn(uint256[] storage a) external view returns (uint256[] storage) {
        require(a.length > 0);
        return a;
    }

    // A mapping is storage-only, so returning one is omitted as well. The ABI printer never
    // sees the mapping type.
    function mappingReturn(Set storage s)
        external
        view
        returns (mapping(uint256 => uint256) storage)
    {
        return s.idx;
    }

    // A struct holding a mapping is storage-only in both directions.
    function setReturn(Set storage s) external pure returns (Set storage) {
        return s;
    }

    // A bare mapping parameter with a mapping return is the same case without a struct.
    function mapOf(mapping(uint256 => uint256) storage m)
        external
        pure
        returns (mapping(uint256 => uint256) storage)
    {
        return m;
    }

    function ext(uint256 x) external pure returns (uint256) {
        return x;
    }

    function memStruct(Plain memory p) external pure returns (uint256) {
        return p.a;
    }
}
