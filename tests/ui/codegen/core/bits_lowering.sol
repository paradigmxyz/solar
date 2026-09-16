//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Bits.leadingZeros` is the `clz` instruction where the target has it and
// its binary-search body elsewhere; `trailingZeros` and `popCount` are
// library code over it and over lane sums.
// The three entry points are small enough to be folded into the dispatcher,
// so the checks anchor on the contract's section rather than a function.
// INTRINSIC-LABEL: bits_lowering.sol:Test
// INTRINSIC: clz
// PORTABLE-LABEL: bits_lowering.sol:Test
// PORTABLE-NOT: clz
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
