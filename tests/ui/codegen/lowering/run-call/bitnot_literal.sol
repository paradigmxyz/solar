//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: lowByte 0x123456 => 0x34
//@ run-call: lowShort 0x123456 => 0x3456
//@ run-call: lowAddress 0x1111111111111111111111111111111111111111111111111111111111111111 => 0x1111111111111111111111111111111111111111
//@ run-call: allOnes => -1
//@ run-call: doubleNot 0x1234 => 0x34

// `~` on an integer literal yields `-x - 1`, so `~(~0 << 8)` is the positive literal 255.
contract BitNotLiteral {
    function lowByte(uint256 x) external pure returns (uint8) {
        return uint8((x >> 8) & (~(~0 << 8)));
    }

    function lowShort(uint256 x) external pure returns (uint16) {
        return uint16(x & (~(~0 << 16)));
    }

    function lowAddress(uint256 x) external pure returns (address) {
        return address(uint160(x & ~((~0x00 >> 160) << 160)));
    }

    function allOnes() external pure returns (int8) {
        return ~0;
    }

    function doubleNot(uint256 x) external pure returns (uint256) {
        return x & ~~255;
    }
}
