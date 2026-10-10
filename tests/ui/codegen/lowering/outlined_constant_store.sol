//@ run-call: OutlinedNarrowing::narrow8 255 => 255
//@ run-call-fail: OutlinedNarrowing::narrow8 256 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002553616665436173743a2076616c756520646f65736e27742066697420696e20382062697473000000000000000000000000000000000000000000000000000000
//@ run-call: OutlinedNarrowing::narrow32 4294967295 => 4294967295
//@ run-call-fail: OutlinedNarrowing::narrow32 4294967296 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002653616665436173743a2076616c756520646f65736e27742066697420696e20333220626974730000000000000000000000000000000000000000000000000000
//@ run-call: OutlinedNarrowing::narrow128 340282366920938463463374607431768211455 => 340282366920938463463374607431768211455
//@ run-call-fail: OutlinedNarrowing::narrow128 340282366920938463463374607431768211456 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002753616665436173743a2076616c756520646f65736e27742066697420696e20313238206269747300000000000000000000000000000000000000000000000000
//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zevm-ir-pipeline=outline -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: first 128 => 7
//@ run-call: second 128 => 9
//@ run-call: first 255 => 7
//@ run-call: second 160 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
//@ run-call: first 0 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
//@ run-call: second 0 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef

// A fixed store is a short but expensive sequence. The later potentially aliasing write prevents
// the load from being forwarded, so outlining must preserve both the store and the caller's input.
contract OutlinedConstantStore {
    // CHECK-LABEL: @module OutlinedConstantStore_runtime
    // CHECK: calldatasize
    // CHECK: push 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
    // CHECK-NOT: push 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
    function first(uint256 pointer) external pure returns (uint256 value) {
        assembly {
            mstore(128, 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
            mstore(and(pointer, 128), 7)
            value := mload(128)
        }
    }

    function second(uint256 pointer) external pure returns (uint256 value) {
        assembly {
            mstore(128, 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
            mstore(and(pointer, 160), 9)
            value := mload(128)
        }
    }
}

contract OutlinedNarrowing {
    function narrow8(uint256 value) external pure returns (uint8) {
        require(value <= type(uint8).max, "SafeCast: value doesn't fit in 8 bits");
        return uint8(value);
    }
    function narrow32(uint256 value) external pure returns (uint32) {
        require(value <= type(uint32).max, "SafeCast: value doesn't fit in 32 bits");
        return uint32(value);
    }
    function narrow128(uint256 value) external pure returns (uint128) {
        require(value <= type(uint128).max, "SafeCast: value doesn't fit in 128 bits");
        return uint128(value);
    }
}
