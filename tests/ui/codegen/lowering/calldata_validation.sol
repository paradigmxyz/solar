//@run-call: nestedFixed [[1, 2], [3, 4]], 9 => 19
//@run-call: dynamicFixed [0x12, 0x3456], 7 => 10
//@run-call: rejectsDirtyBoolArray() => true
//@run-call: rejectsDirtyBoolArrayEncoding() => true

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

    function nestedFixed(uint256[2][2] calldata values, uint256 marker)
        external
        pure
        returns (uint256)
    {
        return values[0][0] + values[0][1] + values[1][0] + values[1][1] + marker;
    }

    function dynamicFixed(bytes[2] calldata values, uint256 marker)
        external
        pure
        returns (uint256)
    {
        return values[0].length + values[1].length + marker;
    }

    function boolMemory(bool[] memory values) external pure returns (bool) {
        return values[0];
    }

    function rejectsDirtyBoolArray() external returns (bool rejected) {
        assembly {
            let payload := mload(0x40)
            mstore(payload, shl(224, 0x66229b79))
            mstore(add(payload, 4), 32)
            mstore(add(payload, 36), 1)
            mstore(add(payload, 68), 2)
            rejected := iszero(call(gas(), address(), 0, payload, 100, 0, 0))
        }
    }

    function reencodeBool(bool[] calldata values) external pure returns (bytes memory) {
        return abi.encode(values);
    }

    function rejectsDirtyBoolArrayEncoding() external returns (bool rejected) {
        assembly {
            let payload := mload(0x40)
            mstore(payload, shl(224, 0xe5cd4dbb))
            mstore(add(payload, 4), 32)
            mstore(add(payload, 36), 1)
            mstore(add(payload, 68), 2)
            rejected := iszero(call(gas(), address(), 0, payload, 100, 0, 0))
        }
    }
}
