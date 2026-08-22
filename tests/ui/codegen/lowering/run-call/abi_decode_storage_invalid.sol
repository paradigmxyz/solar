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
//@[none, gas, size] run-call-fail: decode(bytes) 0x0000000000000000000000000000000000000000000000000000000000000020

contract AbiDecodeStorageInvalid {
    bytes private data;

    function decode(bytes memory input) external returns (uint256) {
        data = input;
        uint256[] memory decoded = abi.decode(data, (uint256[]));
        return decoded.length;
    }
}
