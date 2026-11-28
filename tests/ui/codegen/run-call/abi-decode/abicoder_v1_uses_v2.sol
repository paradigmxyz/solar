//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: sum (4, 5) => 9
//@ run-call: g true => true
//@ run-call-fail: 0xd48092f70000000000000000000000000000000000000000000000000000000000000002

// ABI coder v1 rejects struct parameters and accepts dirty `bool` arguments;
// we compile this as ABI coder v2.
pragma abicoder v1; //~ WARN: ABI coder v1 is not supported

contract AbicoderV1UsesV2 {
    struct S {
        uint256 a;
        uint256 b;
    }

    function sum(S calldata s) external pure returns (uint256) {
        return s.a + s.b;
    }

    function g(bool b) external pure returns (bool) {
        return b;
    }
}
