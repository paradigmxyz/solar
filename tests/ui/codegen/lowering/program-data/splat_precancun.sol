//@ codegen-matrix: standard optimized
//@ compile-flags: --evm-version paris
//@[optimized] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[optimized] filecheck:
//@ run-call: hash => 0x04cea613ff619f915864b8d95704e4704cdb3947a5b6f98be176f30db7333a2f
//@ run-call: paddingIsZero => true

// CHECK-LABEL: @module C_runtime
// CHECK-NOT: push_data literal
contract C {
    function data() internal pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff0011";
    }

    function hash() external pure returns (bytes32) {
        return keccak256(data());
    }

    function paddingIsZero() external pure returns (bool result) {
        bytes memory value = data();
        assembly {
            result := iszero(shr(8, mload(add(add(value, 32), mload(value)))))
        }
    }
}
