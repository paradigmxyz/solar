//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: f() => 8, 3, 9, 11
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_from_storage_struct_v2.sol

contract AbiDecodeStorageStruct {
    bytes data;
    struct S {
        uint256 a;
        uint256[] b;
    }

    function f() external returns (uint256 a, uint256 length, uint256 first, uint256 last) {
        S memory s;
        s.a = 8;
        s.b = new uint256[](3);
        s.b[0] = 9;
        s.b[1] = 10;
        s.b[2] = 11;
        data = abi.encode(s);
        S memory decoded = abi.decode(data, (S));
        return (decoded.a, decoded.b.length, decoded.b[0], decoded.b[2]);
    }
}
