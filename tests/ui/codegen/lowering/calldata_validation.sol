//@revisions: built abi
//@[built] compile-flags: -O none -Zdump=mir
//@[abi] compile-flags: -O none -Zmir-pipeline=lower-abi -Zdump=mir
//@[abi] filecheck:

// Pins the calldata lower-bound check and validators emitted for value-type
// external parameters.
// Like solc via-ir, every value-type argument word must be its canonical ABI
// encoding or the call reverts with empty revert data (`revert(0, 0)`):
// - uintN: high bits zero (`iszero(shr(bits, word))`)
// - intN: word equals its sign extension (`eq(word, signextend(N/8-1, word))`)
// - address/contract: top 96 bits zero
// - bool: word is less than 2 (`lt(word, 2)`)
// - bytesN: low 32-N bytes zero (`iszero(shl(bits, word))`)
// - enum: word < member count (`lt(word, count)`)
// Full-word types (uint256, int256, bytes32) need no canonicalization
// validator, but still need the short-calldata guard. The validators read the
// raw word through the semantic calldata-slice boundary so optimization
// passes may assume `Arg` values of external functions are canonical.
contract CalldataValidation {
    enum Dir {
        Up,
        Down,
        Left
    }

    // CHECK-LABEL: fn @vUint8{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[HIGH:v[0-9]+]] = shr 8, [[RAW]]
    // CHECK: iszero [[HIGH]]
    function vUint8(uint8 x) external pure returns (uint8) {
        return x;
    }

    // CHECK-LABEL: fn @vInt16{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[CANON:v[0-9]+]] = signextend 1, [[RAW]]
    // CHECK: eq [[RAW]], [[CANON]]
    function vInt16(int16 x) external pure returns (int16) {
        return x;
    }

    // CHECK-LABEL: fn @vBool{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: {{v[0-9]+}} = lt [[RAW]], 2
    function vBool(bool x) external pure returns (bool) {
        return x;
    }

    // CHECK-LABEL: fn @vAddress{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[HIGH:v[0-9]+]] = shr 160, [[RAW]]
    // CHECK: iszero [[HIGH]]
    function vAddress(address x) external pure returns (address) {
        return x;
    }

    // CHECK-LABEL: fn @vBytes4{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[LOW:v[0-9]+]] = shl 32, [[RAW]]
    // CHECK: iszero [[LOW]]
    function vBytes4(bytes4 x) external pure returns (bytes4) {
        return x;
    }

    // CHECK-LABEL: fn @vEnum{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: lt [[RAW]], 3
    function vEnum(Dir x) external pure returns (Dir) {
        return x;
    }

    // A canonical narrow input is already valid when returned through a
    // wider unsigned ABI type, so no second cleanup is needed.
    // CHECK-LABEL: fn @vWidened{{[( ]}}
    // CHECK-NOT: and arg0
    // CHECK: mstore 128, arg0
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: {{v[0-9]+}} = shr 8, [[RAW]]
    function vWidened(uint8 x) external pure returns (uint16) {
        return x;
    }

    // CHECK-LABEL: fn @vMulti{{[( ]}}
    // CHECK: [[A:v[0-9]+]] = calldataload 4
    // CHECK: {{v[0-9]+}} = shr 32, [[A]]
    // CHECK: [[B:v[0-9]+]] = calldataload 36
    // CHECK: {{v[0-9]+}} = signextend 0, [[B]]
    function vMulti(uint32 a, int8 b) external pure returns (uint256) {
        return uint256(uint32(a)) + uint256(uint8(int8(b)));
    }

    // Full-word value types are canonical by construction: no validator.
    // CHECK-LABEL: fn @vFull{{[( ]}}
    // CHECK: {{v[0-9]+}} = lt {{v[0-9]+}}, 100
    // CHECK-NOT: calldataload
    // CHECK: add arg0, arg1
    function vFull(uint256 a, bytes32 b, int256 c) external pure returns (uint256) {
        return a + uint256(b) + uint256(c);
    }
}
