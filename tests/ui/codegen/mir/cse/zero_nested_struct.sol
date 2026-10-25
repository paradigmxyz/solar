//@ codegen-matrix: standard
//@ run-call: get 0x0000000000000000000000000000000000001234 => ((0), 0, 0, 0, 0, 4660, 0x0000000000000000000000000000000000000000, 0)
//@ run-call: get 0xffffffffffffffffffffffffffffffffffffffff => ((0), 0, 0, 0, 0, 65535, 0x0000000000000000000000000000000000000000, 0)
//@ run-call: fill 0x0000000000000000000000000000000000001234, 7 => ((7), 7, 8, 9, 7, 4660, 0x0000000000000000000000000000000000001234, 7)

contract ZeroNested {
    struct Config {
        uint256 data;
    }

    struct S {
        Config configuration;
        uint128 a;
        uint128 b;
        uint128 c;
        uint40 d;
        uint16 id;
        address x;
        uint8 e;
    }

    // The nested `Config` is allocated after the zero fill of the outer struct, and the return
    // encoding reloads every field. The first allocation lies above the heap start, so the
    // free-memory pointer write of the second cannot change its words.
    function get(address asset) external pure returns (S memory data) {
        data.id = uint16(uint160(asset));
    }

    function fill(address asset, uint256 v) external pure returns (S memory data) {
        data.configuration.data = v;
        data.a = uint128(v);
        data.b = uint128(v + 1);
        data.c = uint128(v + 2);
        data.d = uint40(v);
        data.id = uint16(uint160(asset));
        data.x = asset;
        data.e = uint8(v);
    }
}
