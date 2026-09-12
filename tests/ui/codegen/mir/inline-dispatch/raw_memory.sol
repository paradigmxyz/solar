//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@ run-call: RawMemory::raw 160 => 0
//@ run-call: RawMemory::sum 17, 23 => 40
//@ run-call-fail: RawRevert::raw 160 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: RawRevert::sum 17, 23 => 40

// A raw return may inspect memory outside the declared ABI return buffer.
// Hoisting another route's arguments must not expose its spill stores there.
// CHECK-LABEL: @module RawMemory
// CHECK: tail_call @raw
contract RawMemory {
    function raw(uint256 offset) external pure returns (uint256) {
        assembly { return(offset, 32) }
    }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }
}

// CHECK-LABEL: @module RawRevert
// CHECK: tail_call @raw
contract RawRevert {
    function raw(uint256 offset) external pure {
        assembly { revert(offset, 32) }
    }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }
}
