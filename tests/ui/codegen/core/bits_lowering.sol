//@ revisions: intrinsic portable cancun
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE
//@[cancun] compile-flags: -Ogas -Zdump=mir --evm-version=cancun
//@[cancun] filecheck: --check-prefix=CANCUN

// `Bits.leadingZeros`, `highestSetBit` and `trailingZeros` are the `clz`
// instruction where the target has it and the table search elsewhere;
// `popCount` is lane sums on every target. The entry points are small enough
// to be folded into the dispatcher, so the checks anchor on the contract's
// section rather than a function.
// INTRINSIC-LABEL: bits_lowering.sol:Test
// INTRINSIC: clz
// INTRINSIC-NOT: 0x8421084210842108cc6318c6db6d54be
// PORTABLE-LABEL: bits_lowering.sol:Test
// PORTABLE-NOT: clz
// PORTABLE: 0x8421084210842108cc6318c6db6d54be
// CANCUN-LABEL: bits_lowering.sol:Test
// CANCUN-NOT: clz
// CANCUN: 0x8421084210842108cc6318c6db6d54be
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
}
