//@compile-flags: --emit=hashes
//@filecheck: --implicit-check-not=get(

// A `library` (unlike a contract) may expose `public`/`external` functions
// that take `storage` reference parameters and refer to structs and enums by
// canonical name. solc encodes their signatures — and hence 4-byte
// selectors — with the type's canonical name and a trailing `storage`
// location suffix on storage reference parameters (`memory`/`calldata`
// parameters carry no suffix): `total(S storage)`, not a flattened
// `total((uint256,uint256))`, and `libEnum(L.Kind)`, not `libEnum(uint8)`.
//
// Contract function signatures are unaffected: structs still flatten into
// ABI tuples, enums encode as `uint8`, and there is no location suffix.
//
// All selectors below match solc 0.8.30.

struct S {
    uint256 a;
    uint256 b;
}

enum Color {
    Red,
    Green
}

contract D {
    struct U {
        uint128 x;
    }

    enum Mode {
        A,
        B
    }
}

// CHECK: :D":{"hashes":{}}

library L {
    struct T {
        uint64 y;
    }

    enum Kind {
        K1,
        K2
    }

    // `get` is dropped from the interface (and hence from `hashes`) because
    // its mapping parameter is not considered exportable yet (see the
    // `interfaceType` TODO in `interface_functions`), which the
    // `--implicit-check-not` above pins. solc lists it, and this printer
    // already produces its signature; once `interface_functions` learns
    // mapping parameters, rename `TODO-CHECK` to `CHECK`:
    // TODO-CHECK: "get(mapping(address => S) storage,address)":"2aed1630"
    function get(mapping(address => S) storage m, address k) public view returns (uint256) {
        return m[k].a;
    }

    // File-level enum by bare canonical name.
    // CHECK: "fileEnum(Color)":"83ef0b32"
    function fileEnum(Color c) external pure returns (uint8) {
        return uint8(c);
    }

    // A `memory` struct is still printed by name, with no location suffix.
    // CHECK: "fileStruct(S)":"bb1da689"
    function fileStruct(S memory s) external pure returns (uint256) {
        return s.a;
    }

    // Struct defined inside the library itself.
    // CHECK: "inLib(L.T storage)":"b9de8475"
    function inLib(T storage t) external view returns (uint64) {
        return t.y;
    }

    // Struct defined inside another contract.
    // CHECK: "inOther(D.U storage)":"131979c8"
    function inOther(D.U storage u) external view returns (uint128) {
        return u.x;
    }

    // Enum defined inside the library itself.
    // CHECK: "libEnum(L.Kind)":"f4df06a2"
    function libEnum(Kind k) external pure returns (uint8) {
        return uint8(k);
    }

    // Enum defined inside another contract.
    // CHECK: "otherEnum(D.Mode)":"7a6eb876"
    function otherEnum(D.Mode m) external pure returns (uint8) {
        return uint8(m);
    }

    // File-level struct in `storage`.
    // CHECK: "total(S storage)":"33ad6f28"
    function total(S storage s) external view returns (uint256) {
        return s.a + s.b;
    }
}
