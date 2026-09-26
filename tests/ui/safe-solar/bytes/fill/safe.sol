//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: blank 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489ae, 3, 33 => 0x0b30552a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a3f6489ae
//@ run-call: blank 0x5555555555555555, 8, 0 => 0x5555555555555555
//@ run-call-fail: blank 0x5555555555555555, 6, 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A fill that checks its range, then stores the byte broadcast to a word once
// per whole word and merges the partial last word under a mask.
// CHECK-LABEL: fn @blank
// CHECK: mstore {{.*}}, 0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a
// CHECK-NOT: mstore8
// CHECK-NOT: icall @fill
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    function blank(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes memory) {
        Bytes.fill(b, offset, count, 0x2a);
        return b;
    }
}
