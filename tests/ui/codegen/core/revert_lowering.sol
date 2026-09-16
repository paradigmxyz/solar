//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Revert.raw` reverts with its argument as the whole payload, no encoding
// around it. The intrinsic is the revert terminator over the buffer's range;
// the shipped body is the same terminator written in assembly.
// INTRINSIC-LABEL: fn @boom
// INTRINSIC: revert {{v[0-9]+}}, {{v[0-9]+}}
// INTRINSIC-NOT: icall @raw
// PORTABLE-LABEL: fn @boom
// PORTABLE: revert
import {Revert} from "solar:core/v1/Revert.sol";

contract Test {
    function boom(bytes memory data) public pure {
        Revert.raw(data);
    }

    function maybe(bytes memory data, bool go) public pure returns (uint256) {
        if (go) Revert.raw(data);
        return 7;
    }
}
