//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

import {Arrays} from "solar:core/Arrays.sol";

contract Test {
    using Arrays for uint256[];

    // Truncation is a check against the current length and one store of the
    // new one. The length read after it folds to the stored value, which is
    // the alias model working: a stale length is never kept across the store.
    // The shipped body inlines to the same check and store, and the length read
    // after its assembly store folds to the stored value as well.
    // INTRINSIC-LABEL: fn @shorten
    // INTRINSIC: [[LEN:v[0-9]+]] = calldataload v{{[0-9]+}}
    // INTRINSIC: gt arg1, [[LEN]]
    // INTRINSIC: mstore {{v[0-9]+}}, arg1
    // INTRINSIC-NOT: icall @truncate
    // PORTABLE-LABEL: fn @shorten
    // PORTABLE: mstore {{v[0-9]+}}, arg1
    // PORTABLE-NEXT: mstore 0, {{v[0-9]+}}
    // PORTABLE-NEXT: mstore 32, arg1
    function shorten(uint256[] memory a, uint256 n) public pure returns (uint256, uint256) {
        uint256 before = a.length;
        a.truncate(n);
        return (before, a.length);
    }
}
