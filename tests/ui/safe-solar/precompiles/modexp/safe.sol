//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: power 0x03, 0x05, 0x07 => true, 0x05
//@ run-call: power 0x02, 0x0a, 0x03e8 => true, 0x0018
//@ run-call: power 0x, 0x, 0x => true, 0x

// Modular exponentiation over integers of any length; the result has the
// modulus's length.
// CHECK-LABEL: fn @power
// CHECK: staticcall {{v[0-9]+}}, 5,
import {Precompiles} from "solar:core/v1/Precompiles.sol";

contract Safe {
    function power(bytes memory base, bytes memory exponent, bytes memory modulus)
        public
        view
        returns (bool, bytes memory)
    {
        return Precompiles.modexp(base, exponent, modulus);
    }
}
