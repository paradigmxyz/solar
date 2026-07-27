//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Pins the calldata lower-bound check and validators emitted for value-type
// external parameters.
// Like solc via-ir, every value-type argument word must be its canonical ABI
// encoding or the call reverts with empty revert data (`revert(0, 0)`):
// - uintN: high bits zero (`eq(word, and(word, mask))`)
// - intN: word equals its sign extension (`eq(word, signextend(N/8-1, word))`)
// - address/contract: top 96 bits zero
// - bool: word is 0 or 1 (`eq(word, iszero(iszero(word)))`)
// - bytesN: low 32-N bytes zero
// - enum: word < member count (`lt(word, count)`)
// Full-word types (uint256, int256, bytes32) need no canonicalization
// validator, but still need the short-calldata guard. The validators read the
// raw word with an explicit `calldataload` so optimization passes may assume
// `Arg` values of external functions are canonical.
contract CalldataValidation {
    enum Dir {
        Up,
        Down,
        Left
    }

    // CHECK-LABEL: fn @vUint8{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[CANON:v[0-9]+]] = and [[RAW]], 255
    // CHECK: eq [[RAW]], [[CANON]]
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
    // CHECK: [[ZERO:v[0-9]+]] = iszero [[RAW]]
    // CHECK: [[CANON:v[0-9]+]] = iszero [[ZERO]]
    // CHECK: eq [[RAW]], [[CANON]]
    function vBool(bool x) external pure returns (bool) {
        return x;
    }

    // CHECK-LABEL: fn @vAddress{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[CANON:v[0-9]+]] = and [[RAW]], 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: eq [[RAW]], [[CANON]]
    function vAddress(address x) external pure returns (address) {
        return x;
    }

    // CHECK-LABEL: fn @vBytes4{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: [[CANON:v[0-9]+]] = and [[RAW]], 0xffffffff00000000000000000000000000000000000000000000000000000000
    // CHECK: eq [[RAW]], [[CANON]]
    function vBytes4(bytes4 x) external pure returns (bytes4) {
        return x;
    }

    // CHECK-LABEL: fn @vEnum{{[( ]}}
    // CHECK: [[RAW:v[0-9]+]] = calldataload 4
    // CHECK: lt [[RAW]], 3
    function vEnum(Dir x) external pure returns (Dir) {
        return x;
    }

    // CHECK-LABEL: fn @vMulti{{[( ]}}
    // CHECK: [[A:v[0-9]+]] = calldataload 4
    // CHECK: and [[A]], 0xffffffff
    // CHECK: [[B:v[0-9]+]] = calldataload 36
    // CHECK: signextend 0, [[B]]
    function vMulti(uint32 a, int8 b) external pure returns (uint256) {
        return uint256(uint32(a)) + uint256(uint8(int8(b)));
    }

    // Full-word value types are canonical by construction: no validator.
    // CHECK-LABEL: fn @vFull{{[( ]}}
    // CHECK: {{v[0-9]+}} = slt {{v[0-9]+}}, 96
    // CHECK-NOT: calldataload
    // CHECK: add arg0, arg1
    function vFull(uint256 a, bytes32 b, int256 c) external pure returns (uint256) {
        return a + uint256(b) + uint256(c);
    }
}
