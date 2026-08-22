//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0x9a869a7400000000000000000000000000000000000000000000000000000000000ff01000000000000000000000000000000000000000000000000000000000000000ff6162000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff010000000000000000000000000000000000000000000000000000000000ff00026162000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0x9a869a74ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0100000000000000000000000000000000000000000000000000000000000000ff6162636400000000000000000000000000000000000000000000000000000000
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
