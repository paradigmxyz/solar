//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: encode => 0x0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_rational.sol

contract AbiEncodeRational {
    function encode() external pure returns (bytes memory) {
        return abi.encode(1, -2);
    }
}
