//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: encode() => 0x0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_rational.sol

contract AbiEncodeRational {
    function encode() external pure returns (bytes memory) {
        return abi.encode(1, -2);
    }
}
