//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f() => 0x616263646566, 0x31323334353637383930313233343536373839303132333435363738393031203132333435363738393031323334353637383930313233343536373839303120313233343536373839
//@ run-call: g() => 0x31323334353637383930313233343536373839303132333435363738393031203132333435363738393031323334353637383930313233343536373839303120313233343536373839, 0x3132333435363738393233343536373839
//@ run-call: h() => 0x, 0x
// ported-from: test/libsolidity/semanticTests/array/copying/copy_byte_array_in_struct_to_storage.sol

pragma abicoder v2;

contract StorageStructTwoBytes {
    struct Entry {
        uint16 x;
        bytes a;
        uint16 y;
        bytes b;
    }

    uint256 padding;
    Entry data;

    function f() external returns (bytes memory, bytes memory) {
        Entry memory value;
        value.x = 7;
        value.b = "1234567890123456789012345678901 1234567890123456789012345678901 123456789";
        value.a = "abcdef";
        value.y = 9;
        data = value;
        return (data.a, data.b);
    }

    function g() external returns (bytes memory, bytes memory) {
        Entry memory value;
        value.x = 7;
        value.b = "12345678923456789";
        value.a = "1234567890123456789012345678901 1234567890123456789012345678901 123456789";
        value.y = 9;
        data = value;
        return (data.a, data.b);
    }

    function h() external returns (bytes memory, bytes memory) {
        Entry memory value;
        data = value;
        return (data.a, data.b);
    }
}
