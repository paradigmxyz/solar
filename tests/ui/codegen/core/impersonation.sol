//@ compile-flags: -Ogas -Zdump=mir solar:core/=tests/ui/codegen/core/auxiliary/fake/
//@ filecheck:
//@ run-call: f 0x11223344556677 => 0x11223344

// The reserved prefix is intercepted before any file resolution runs, so a
// remapping aimed at it resolves to the compiler's module anyway. Reaching the
// stand-in would return `0xdeadbeef` and skip the bounds check with it.
// CHECK-LABEL: fn @f
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// CHECK-NOT: deadbeef
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    function f(bytes memory b) public pure returns (bytes4) {
        return Bytes.readBytes4(b, 0);
    }
}
