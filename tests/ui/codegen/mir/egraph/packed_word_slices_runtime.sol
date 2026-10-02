//@ codegen-matrix: standard
//@ run-call: discardMasks 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff00000000000000000000000000000000000000000000000000000000000000, 255, 255
//@ run-call: discardMasks 256 => 0, 0, 0
//@ run-call: adjustBytes 0xab000000000000000000000000000000000000000000000000000000000000cd => 205, 171, 0
//@ run-call: adjustBytes 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255, 255, 0
//@ run-call: preserveBits 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x7f00000000000000000000000000000000000000000000000000000000000000, 127, 255, 0
//@ run-call: preserveBits 1 => 0x0100000000000000000000000000000000000000000000000000000000000000, 1, 0, 0
contract BitSlices {
    function discardMasks(uint256 x) external pure returns (uint256 left, uint256 right, uint256 last) {
        assembly {
            left := shl(248, and(x, 255))
            right := shr(248, and(x, shl(248, 255)))
            last := byte(31, and(255, x))
        }
    }

    function adjustBytes(uint256 x) external pure returns (uint256 low, uint256 high, uint256 zero) {
        assembly {
            low := byte(0, shl(248, x))
            high := byte(31, shr(248, x))
            zero := byte(0, shr(8, x))
        }
    }

    function preserveBits(uint256 x) external pure returns (uint256 clipped, uint256 last, uint256 unaligned, uint256 outOfRange) {
        assembly {
            clipped := shl(248, and(x, 127))
            last := byte(31, and(x, 127))
            unaligned := byte(31, shr(7, x))
            outOfRange := byte(not(0), shl(8, x))
        }
    }
}
