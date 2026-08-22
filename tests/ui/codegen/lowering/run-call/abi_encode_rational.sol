//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: encode() => 0x0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_rational.sol

contract AbiEncodeRational {
    function encode() external pure returns (bytes memory) {
        return abi.encode(1, -2);
    }
}
