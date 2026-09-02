//@ compile-flags: --emit=abi,hashes --pretty-json

// A `library` may expose `public`/`external` functions taking `storage`
// reference parameters. Storage references have no ABI encoding, so solc
// omits those functions from the JSON ABI, but still lists them in the
// method identifiers with a `storage` location suffix.

library L {
    struct Set {
        mapping(uint256 => uint256) idx;
    }

    struct Plain {
        uint256 a;
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

    function ext(uint256 x) external pure returns (uint256) {
        return x;
    }

    function memStruct(Plain memory p) external pure returns (uint256) {
        return p.a;
    }
}
