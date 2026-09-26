//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Math.mul512` is a `mul`, a `mulmod` by the largest word and the borrow
// between them, with both results handed back as values, not through memory.
// The wrapping operations are the instruction they name.
// INTRINSIC-LABEL: fn @mul512
// INTRINSIC: mulmod {{v[0-9]+|arg[0-9]+}}, {{v[0-9]+|arg[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
// INTRINSIC-NOT: icall @mul512
// PORTABLE-LABEL: fn @mul512
// PORTABLE: mulmod
import {Math, Rounding} from "solar:core/v1/Math.sol";

contract Test {
    function mul512(uint256 x, uint256 y) public pure returns (uint256 high, uint256 low) {
        return Math.mul512(x, y);
    }

    function mulDiv(uint256 x, uint256 y, uint256 denominator, bool up) public pure returns (uint256) {
        return Math.mulDiv(x, y, denominator, up ? Rounding.Up : Rounding.Down);
    }

    function wrapAdd(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingAdd(x, y);
    }

    function wrapSub(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingSub(x, y);
    }

    function wrapMul(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingMul(x, y);
    }
}
