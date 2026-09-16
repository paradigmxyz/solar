//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: leading 0 => 256
//@ run-call: leading 1 => 255
//@ run-call: leading 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: leading 255 => 248
//@ run-call: trailing 0 => 256
//@ run-call: trailing 1 => 0
//@ run-call: trailing 256 => 8
//@ run-call: trailing 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 255
//@ run-call: ones 0 => 0
//@ run-call: ones 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 256
//@ run-call: ones 1 => 1
//@ run-call: ones 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 1
//@ run-call: ones 115792089237316195423570985008687907853269984665640564039457584007913129639934 => 255
//@ run-call: ones 61680 => 8

// `Bits.leadingZeros` is the `clz` instruction where the target has it and
// its binary-search body elsewhere; `trailingZeros` and `popCount` are
// library code over it and over lane sums.
import {Bits} from "solar:core/v1/Bits.sol";

contract Test {
    function leading(uint256 x) public pure returns (uint256) {
        return Bits.leadingZeros(x);
    }

    function trailing(uint256 x) public pure returns (uint256) {
        return Bits.trailingZeros(x);
    }

    function ones(uint256 x) public pure returns (uint256) {
        return Bits.popCount(x);
    }
}
