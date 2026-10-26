//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: short => 0x11223300
//@ run-call: long => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: convertedString => 0x6162000000000000000000000000000000000000000000000000000000000000
//@ run-call: convertedString31 => 0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333400
//@ run-call: convertedString32 => 0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435
//@ run-call: convertedLongString => 0x3031323334353637383961626364656630313233343536373839616263646566
//@ run-call: storagePointer => 0x11223300
//@ run-call: structMember => 0xaabb
//@ run-call: arrayElement => 0x0102

contract StorageBytesToFixed {
    struct Wrapper {
        bytes c;
    }

    bytes stored;
    string text;
    Wrapper wrapper;
    bytes[] items;

    function short() external returns (bytes4) {
        stored = hex"112233";
        return bytes4(stored);
    }

    function long() external returns (bytes32) {
        stored = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        return bytes32(stored);
    }

    // The conversion reads the storage value itself, not its slot, for every argument form and
    // on both sides of the 31-byte short encoding limit.
    function convertedString() external returns (bytes32) {
        text = "ab";
        return bytes32(bytes(text));
    }

    function convertedString31() external returns (bytes32) {
        text = "abcdefghijklmnopqrstuvwxyz01234";
        return bytes32(bytes(text));
    }

    function convertedString32() external returns (bytes32) {
        text = "abcdefghijklmnopqrstuvwxyz012345";
        return bytes32(bytes(text));
    }

    function convertedLongString() external returns (bytes32) {
        text = "0123456789abcdef0123456789abcdefX";
        return bytes32(bytes(text));
    }

    function storagePointer() external returns (bytes4) {
        bytes storage p = stored;
        stored = hex"112233";
        return bytes4(p);
    }

    function structMember() external returns (bytes2) {
        wrapper.c = hex"aabb";
        return bytes2(wrapper.c);
    }

    function arrayElement() external returns (bytes2) {
        items.push(hex"0102");
        return bytes2(items[0]);
    }
}
