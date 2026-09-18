//@ revisions: intrinsic portable cancun
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[cancun] compile-flags: -Ogas --evm-version=cancun
//@ run-call: leading 0 => 256
//@ run-call: leading 1 => 255
//@ run-call: leading 2 => 254
//@ run-call: leading 255 => 248
//@ run-call: leading 340282366920938463463374607431768211455 => 128
//@ run-call: leading 340282366920938463463374607431768211456 => 127
//@ run-call: leading 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: leading 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: highest 0 => 256
//@ run-call: highest 1 => 0
//@ run-call: highest 3 => 1
//@ run-call: highest 255 => 7
//@ run-call: highest 256 => 8
//@ run-call: highest 340282366920938463463374607431768211455 => 127
//@ run-call: highest 340282366920938463463374607431768211456 => 128
//@ run-call: highest 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 255
//@ run-call: trailing 0 => 256
//@ run-call: trailing 1 => 0
//@ run-call: trailing 6 => 1
//@ run-call: trailing 256 => 8
//@ run-call: trailing 340282366920938463463374607431768211456 => 128
//@ run-call: trailing 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 255
//@ run-call: trailing 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: ones 0 => 0
//@ run-call: ones 1 => 1
//@ run-call: ones 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 1
//@ run-call: ones 115792089237316195423570985008687907853269984665640564039457584007913129639934 => 255
//@ run-call: ones 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 256
//@ run-call: ones 61680 => 8
//@ run-call: sweep => true

// `Bits` scans and counts. On a target with `clz` the three scans are that
// instruction; elsewhere, and under `-Zno-core-intrinsics`, they are a
// branch-free search finished by a table. The third revision compiles for a
// target without the instruction, where the intrinsic has to stand down by
// itself. `sweep` walks every bit position through all four functions.
import {Bits} from "solar:core/v1/Bits.sol";

contract Test {
    function leading(uint256 x) public pure returns (uint256) {
        return Bits.leadingZeros(x);
    }

    function highest(uint256 x) public pure returns (uint256) {
        return Bits.highestSetBit(x);
    }

    function trailing(uint256 x) public pure returns (uint256) {
        return Bits.trailingZeros(x);
    }

    function ones(uint256 x) public pure returns (uint256) {
        return Bits.popCount(x);
    }

    function sweep() public pure returns (bool) {
        uint256 all = type(uint256).max;
        for (uint256 k; k < 256; ++k) {
            uint256 bit = 1 << k;
            if (Bits.leadingZeros(bit) != 255 - k) return false;
            if (Bits.leadingZeros(bit | (bit >> 1) | 1) != 255 - k) return false;
            if (Bits.leadingZeros(all >> k) != k) return false;
            if (Bits.highestSetBit(bit) != k) return false;
            if (Bits.highestSetBit(bit | 1) != k) return false;
            if (Bits.trailingZeros(bit) != k) return false;
            if (Bits.trailingZeros(all << k) != k) return false;
            if (Bits.trailingZeros(bit | (1 << 255)) != k) return false;
            if (Bits.popCount(all >> k) != 256 - k) return false;
            if (Bits.popCount(bit) != 1) return false;
        }
        return true;
    }
}
