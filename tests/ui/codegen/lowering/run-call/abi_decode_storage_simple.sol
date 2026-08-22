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
//@[none, gas, size] run-call: decode(bytes) 0x00000000000000000000000000000000000000000000000000000000000000210000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000761626364656667000000000000000000000000000000000000000000000000000000 => 33, 7, 0x61
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_simple_storage.sol

contract AbiDecodeStorageSimple {
    bytes private data;

    function decode(bytes memory input) external returns (uint256 value, uint256 length, bytes1 first) {
        data = input;
        bytes memory decoded;
        (value, decoded) = abi.decode(data, (uint256, bytes));
        return (value, decoded.length, decoded[0]);
    }
}
