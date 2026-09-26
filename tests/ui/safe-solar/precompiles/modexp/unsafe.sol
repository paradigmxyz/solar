//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: power 0x03, 0x05, 0x07 => true, 0x05
//@ run-call: power 0x02, 0x0a, 0x03e8 => true, 0x0018
//@ run-call: power 0x, 0x, 0x => true, 0x

// The same call in assembly: the three lengths and the three operands laid
// out by hand past the free pointer, the result allocated by moving it.
// CHECK-LABEL: fn @power
// CHECK: staticcall {{v[0-9]+}}, 5,
contract Unsafe {
    function power(bytes memory base, bytes memory exponent, bytes memory modulus)
        public
        view
        returns (bool ok, bytes memory result)
    {
        assembly ("memory-safe") {
            let m := mload(0x40)
            let bl := mload(base)
            let el := mload(exponent)
            let ml := mload(modulus)
            mstore(m, bl)
            mstore(add(m, 0x20), el)
            mstore(add(m, 0x40), ml)
            mcopy(add(m, 0x60), add(base, 0x20), bl)
            mcopy(add(add(m, 0x60), bl), add(exponent, 0x20), el)
            mcopy(add(add(add(m, 0x60), bl), el), add(modulus, 0x20), ml)
            let size := add(add(add(0x60, bl), el), ml)
            result := add(m, size)
            mstore(result, ml)
            ok := staticcall(gas(), 5, m, size, add(result, 0x20), ml)
            mstore(0x40, and(add(add(result, 0x3f), ml), not(0x1f)))
        }
    }
}
