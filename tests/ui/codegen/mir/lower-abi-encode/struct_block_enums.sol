//@ codegen-matrix: standard paris native
//@[paris] compile-flags: -Ogas --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun -Zdump=mir-final
//@[native] filecheck: --check-prefix=NATIVE
//@ run-call: lots 1, 1, 1, 0xdeadbeef => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000112200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000deadbe9800000000000000000000000000000000000000000000000000000000deadbef000000000000000000000000000000000000000000000000000000000000000a1000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001112200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000deadbe99
//@ run-call-fail: lots 3, 1, 0, 0 => Panic(0x21)
//@ run-call-fail: lots 1, 2, 1, 0 => Panic(0x21)
//@ run-call-fail: lots 3, 2, 1, 0 => Panic(0x21)
//@ run-call-fail: lots 0, 2, 0, 0xdeadbeef => Panic(0x21)
//@ run-call-fail: lots 2, 2, 1, 0xdeadbeef => Panic(0x21)
//@ run-call: one 0xdeadbeef => (0xdeadbeef, 0x00000000000000000000000000000000000000a0, 1, true, 1, 0x1122, 3735928472)
//@ run-call-fail: bad 0xdeadbeef => 0xad1e41c900000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001112200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000deadbe98

// With `MCOPY`, a struct of word fields encodes as one block copy, cleans its narrow fields in
// place, and only checks its enums. Each enum out of range, one that follows other fields, and
// one in a later array element must still panic with `Panic(0x21)`, as the per-field path of
// Paris does, whichever enum fails first. The array elements and the revert data copy seven
// words as one block; the fixed return buffer of `one` keeps loading and storing each field.
// NATIVE-LABEL: fn @lots()
// NATIVE: mcopy {{v[0-9]+}}, {{v[0-9]+}}, 224
// NATIVE-LABEL: fn @one()
// NATIVE-NOT: mcopy
// NATIVE-LABEL: fn @bad()
// NATIVE: mcopy {{v[0-9]+}}, {{v[0-9]+}}, 224
contract StructBlockEnums {
    enum Kind { A, B, C }
    enum Level { Low, High }

    struct Lot {
        uint256 id;
        address owner;
        Kind kind;
        bool flag;
        Level level;
        bytes2 tag;
        uint256 amount;
    }

    error Bad(Lot lot);

    function lots(uint256 kind, uint256 level, uint256 at, uint256 dirty)
        external
        pure
        returns (bytes memory)
    {
        Lot[] memory items = new Lot[](2);
        for (uint256 i; i < 2; ++i) {
            Lot memory item = items[i];
            uint256 isAt = i == at ? 1 : 0;
            assembly {
                mstore(item, add(dirty, i))
                mstore(add(item, 32), or(shl(160, dirty), add(0xa0, i)))
                mstore(add(item, 64), mul(kind, isAt))
                mstore(add(item, 96), shl(8, dirty))
                mstore(add(item, 128), mul(level, isAt))
                mstore(add(item, 160), or(shl(240, 0x1122), shr(16, dirty)))
                mstore(add(item, 192), xor(xor(dirty, i), 0x77))
            }
        }
        return abi.encode(items);
    }

    function one(uint256 dirty) external pure returns (Lot memory) {
        return dirtyLot(dirty);
    }

    function bad(uint256 dirty) external pure {
        revert Bad(dirtyLot(dirty));
    }

    function dirtyLot(uint256 dirty) internal pure returns (Lot memory item) {
        Lot[] memory items = new Lot[](1);
        item = items[0];
        assembly {
            mstore(item, dirty)
            mstore(add(item, 32), or(shl(160, dirty), 0xa0))
            mstore(add(item, 64), 1)
            mstore(add(item, 96), shl(8, dirty))
            mstore(add(item, 128), 1)
            mstore(add(item, 160), or(shl(240, 0x1122), shr(16, dirty)))
            mstore(add(item, 192), xor(dirty, 0x77))
        }
    }
}
