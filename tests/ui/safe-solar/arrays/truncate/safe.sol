//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: shorten [1, 2, 3, 4], 2 => [1, 2]
//@ run-call: viaAlias [7, 8, 9], 1 => 1
//@ run-call-fail: shorten [1, 2, 3], 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Shrinking an array in place has no Solidity spelling, so this is the
// compiler-owned one: a check that the array only gets shorter, then one store
// of the length word. Every alias observes it -- `viaAlias` truncates through
// a second reference and reads the length through the first.
// CHECK-LABEL: fn @shorten
// CHECK: gt arg1
// CHECK: mstore {{v[0-9]+}}, arg1
// CHECK-NOT: icall @truncate
import {Arrays} from "solar:core/v1/Arrays.sol";

contract Safe {
    using Arrays for uint256[];

    function shorten(uint256[] memory a, uint256 n) public pure returns (uint256[] memory) {
        a.truncate(n);
        return a;
    }

    function viaAlias(uint256[] memory a, uint256 n) public pure returns (uint256) {
        uint256[] memory b = a;
        b.truncate(n);
        return a.length;
    }
}
