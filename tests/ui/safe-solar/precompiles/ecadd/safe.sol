//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: add 1, 2, 1, 2 => true, 1368015179489954701390400359078579693043519447331113978918064868415326638035, 9918110051302171585080402603319702774565515993150576347155970296011118125764
//@ run-call: add 0, 0, 1, 2 => true, 1, 2
//@ run-call: add 1, 3, 1, 2 => false, 0, 0

// Adding two curve points. A point off the curve is `false` with a zeroed
// result.
// CHECK-LABEL: fn @add
// CHECK: staticcall {{v[0-9]+}}, 6,
import {Precompiles} from "solar:core/v1/Precompiles.sol";

contract Safe {
    function add(uint256 x1, uint256 y1, uint256 x2, uint256 y2)
        public
        view
        returns (bool, uint256, uint256)
    {
        return Precompiles.ecAdd(x1, y1, x2, y2);
    }
}
