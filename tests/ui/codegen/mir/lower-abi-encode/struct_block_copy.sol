//@ codegen-matrix: standard paris native
//@[paris] compile-flags: -Ogas --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun -Zdump=mir-final
//@[native] filecheck: --check-prefix=NATIVE
//@ run-call: encodeItems 2, 0xdeadbeefcafebabe0123456789abcdef00112233445566778899aabbccddeeff => 0x0000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000a2fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd1122334400000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff800000000000000000000000000000000000000000000000000000000000000001deadbeefcafebabe0123456789abcdef00112233445566778899aabbccddef010000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000a0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1122334400000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff800000000000000000000000000000000000000000000000000000000000000001deadbeefcafebabe0123456789abcdef00112233445566778899aabbccddeeff000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000a1fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe1122334400000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff800000000000000000000000000000000000000000000000000000000000000001deadbeefcafebabe0123456789abcdef00112233445566778899aabbccddef00000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000a2fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd1122334400000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff800000000000000000000000000000000000000000000000000000000000000001deadbeefcafebabe0123456789abcdef00112233445566778899aabbccddef01
//@ run-call: literal 5 => 0x0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000500000000000000000000000000000000000000000000000000000000000000051122334400000000000000000000000000000000000000000000000000000000fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000006
//@ run-call-fail: encodeItems 3, 0 => Panic(0x21)
//@ run-call-fail: encodeItems 0x0100000000000000000000000000000000000000000000000000000000000000, 0 => Panic(0x21)

// With `MCOPY`, a memory struct of word fields encodes as one block copy and its narrow fields
// are cleaned in place afterwards. Dirty bits that assembly writes into address, enum, bytes,
// signed, and bool fields must encode as the per-field path, which Paris keeps, cleans them: an
// array element and a single struct alike, and an enum out of range still panics.
// The array element and the single struct each copy seven words, then check the enum and clean
// the address, bytes4, int8, and bool words in place.
// NATIVE-LABEL: fn @encodeItems()
// NATIVE: mcopy [[ITEMA:v[0-9]+]], {{v[0-9]+}}, 224
// NATIVE-NEXT: [[KINDA:v[0-9]+]] = mload [[ITEMA]]
// NATIVE-NEXT: gt [[KINDA]], 2
// NATIVE: [[TOKENA:v[0-9]+]] = add [[ITEMA]], 32
// NATIVE-NEXT: [[DIRTYA:v[0-9]+]] = mload [[TOKENA]]
// NATIVE-NEXT: [[CLEANA:v[0-9]+]] = and [[DIRTYA]], 0xffffffffffffffffffffffffffffffffffffffff
// NATIVE-NEXT: mstore [[TOKENA]], [[CLEANA]]
// NATIVE: and {{v[0-9]+}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// NATIVE: signextend 0,
// NATIVE: ne {{v[0-9]+}}, 0
// NATIVE: mcopy [[ITEMB:v[0-9]+]], {{v[0-9]+}}, 224
// NATIVE-NEXT: [[KINDB:v[0-9]+]] = mload [[ITEMB]]
// NATIVE-NEXT: gt [[KINDB]], 2
// NATIVE: [[TOKENB:v[0-9]+]] = add [[ITEMB]], 32
// NATIVE-NEXT: [[DIRTYB:v[0-9]+]] = mload [[TOKENB]]
// NATIVE-NEXT: [[CLEANB:v[0-9]+]] = and [[DIRTYB]], 0xffffffffffffffffffffffffffffffffffffffff
// NATIVE-NEXT: mstore [[TOKENB]], [[CLEANB]]
// NATIVE: and {{v[0-9]+}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// NATIVE: signextend 0,
// NATIVE: ne {{v[0-9]+}}, 0
// A struct allocated in the function keeps per-field stores.
// NATIVE-LABEL: fn @literal(
// NATIVE-NOT: mcopy {{v[0-9]+}}, {{v[0-9]+}}, 224
contract StructBlockCopy {
    enum Kind { A, B, C }

    struct Item {
        Kind kind;
        address token;
        uint256 id;
        bytes4 tag;
        int8 delta;
        bool flag;
        uint256 amount;
    }

    function encodeItems(uint256 kind, uint256 dirty) external pure returns (bytes memory) {
        Item[] memory items = new Item[](3);
        for (uint256 i; i < 3; ++i) {
            Item memory item = items[i];
            assembly {
                mstore(item, kind)
                mstore(add(item, 32), or(shl(160, dirty), add(0xa0, i)))
                mstore(add(item, 64), not(i))
                mstore(add(item, 96), or(shl(224, 0x11223344), shr(32, dirty)))
                mstore(add(item, 128), or(shl(8, dirty), 0x80))
                mstore(add(item, 160), shl(8, dirty))
                mstore(add(item, 192), add(dirty, i))
            }
        }
        return abi.encode(items, items[2]);
    }

    function literal(uint256 amount) external pure returns (bytes memory) {
        Item memory item = Item(Kind.B, address(uint160(amount)), amount, 0x11223344, -2, true, amount + 1);
        return abi.encode(item);
    }
}
