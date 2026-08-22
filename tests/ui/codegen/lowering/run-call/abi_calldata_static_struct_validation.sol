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
//@[none, gas, size] run-call: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000
//@[none, gas, size] run-call-fail: 0x9a869a7400000000000000000000000000000000000000000000000000000000000ff01000000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000
//@[none, gas, size] run-call-fail: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff010000000000000000000000000000000000000000000000000000000000ff00026162000000000000000000000000000000000000000000000000000000000000
//@[none, gas, size] run-call-fail: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162636400000000000000000000000000000000000000000000000000000000
// ported-from: test/libsolidity/semanticTests/abicoder/validation/static_struct_v2.sol

pragma abicoder v2;

contract AbiCalldataStaticStructValidation {
    struct S {
        int16 a;
        uint8 b;
        bytes2 c;
    }

    function f(S memory s) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := mload(s)
            b := mload(add(s, 0x20))
            c := mload(add(s, 0x40))
        }
    }
}
