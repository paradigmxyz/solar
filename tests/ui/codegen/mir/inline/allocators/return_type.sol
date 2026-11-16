//@ codegen-matrix: standard gas-dump size-dump
//@[gas-dump] compile-flags: -Ogas -Zdump=mir-final
//@[gas-dump] filecheck:
//@[size-dump] compile-flags: -Osize -Zdump=mir-final
//@[size-dump] filecheck:
//@ run-call: Word::one => 0x80
//@ run-call: Word::two => 0x120
//@ run-call: Word::three => 0x1e0
//@ run-call: Bytes::one => 0x80
//@ run-call: Bytes::two => 0x120
//@ run-call: Bytes::three => 0x1e0

// Reduced from https://github.com/paradigmxyz/solar/issues/2643. A bump allocator gets the same
// inlining decision whether it hands its pointer back as a one-word user-defined value type or as
// `bytes memory`: the two bodies differ only by a pointer cast.
type Builder is uint256;

function newBuilder() pure returns (Builder b) {
    assembly ("memory-safe") {
        b := mload(0x40)
        mstore(0x40, add(0x20, b))
    }
}

function newBytes() pure returns (bytes memory b) {
    assembly ("memory-safe") {
        b := mload(0x40)
        mstore(0x40, add(0x20, b))
    }
}

// CHECK-LABEL: @module Word
// CHECK-NOT: icall
// CHECK-NOT: fn @newBuilder
contract Word {
    function one() external pure returns (uint256) {
        return Builder.unwrap(newBuilder());
    }

    function two() external pure returns (uint256) {
        return Builder.unwrap(newBuilder()) + Builder.unwrap(newBuilder());
    }

    function three() external pure returns (uint256) {
        return Builder.unwrap(newBuilder()) + Builder.unwrap(newBuilder())
            + Builder.unwrap(newBuilder());
    }
}

// CHECK-LABEL: @module Bytes
// CHECK-NOT: icall
// CHECK-NOT: fn @newBytes
contract Bytes {
    function one() external pure returns (uint256) {
        return addr(newBytes());
    }

    function two() external pure returns (uint256) {
        return addr(newBytes()) + addr(newBytes());
    }

    function three() external pure returns (uint256) {
        return addr(newBytes()) + addr(newBytes()) + addr(newBytes());
    }

    function addr(bytes memory b) internal pure returns (uint256 p) {
        assembly ("memory-safe") {
            p := b
        }
    }
}
