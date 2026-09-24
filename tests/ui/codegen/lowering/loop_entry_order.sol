//@ compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ filecheck:
//@ run-call: split "a", "a" => ["", ""]
//@ run-call: split "a,b,,c", "," => ["a", "b", "", "c"]
//@ run-call: split "xxabcdefghijklmnopqrstuvwxyz0123456789yy", "abcdefghijklmnopqrstuvwxyz0123456789" => ["xx", "yy"]

// Each search loop carries eight words it never changes below its cursor and
// its output. The block entering a loop holds them in the order its own
// instructions left them, and the loop's layouts take that order: the long
// needle's scan enters its loop right after hashing the needle, copying its
// two starting cursors, where it permuted all eight words before.
// CHECK: keccak256
// CHECK-NEXT: dup {{[0-9]+}}
// CHECK-NEXT: dup {{[0-9]+}}
// CHECK-NEXT: swap 1
// CHECK-NEXT: jump bb{{[0-9]+}}
// CHECK-NEXT: bb{{[0-9]+}} [loop]:
import {Strings} from "solar:core/v1/Strings.sol";

contract Split {
    function split(string memory subject, string memory delimiter)
        public
        pure
        returns (string[] memory)
    {
        return Strings.split(subject, delimiter);
    }
}
