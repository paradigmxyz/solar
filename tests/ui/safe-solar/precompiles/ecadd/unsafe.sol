//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: add 1, 2, 1, 2 => true, 1368015179489954701390400359078579693043519447331113978918064868415326638035, 9918110051302171585080402603319702774565515993150576347155970296011118125764
//@ run-call: add 0, 0, 1, 2 => true, 1, 2
//@ run-call: add 1, 3, 1, 2 => false, 1, 3

// The same call in assembly, with the output written over the input. When
// the precompile rejects the point it writes nothing, and the coordinates
// read back are the rejected input; a caller that does not test the flag
// takes them for the sum.
// CHECK-LABEL: fn @add
// CHECK: staticcall {{v[0-9]+}}, 6,
contract Unsafe {
    function add(uint256 x1, uint256 y1, uint256 x2, uint256 y2)
        public
        view
        returns (bool ok, uint256 x, uint256 y)
    {
        assembly ("memory-safe") {
            let m := mload(0x40)
            mstore(m, x1)
            mstore(add(m, 0x20), y1)
            mstore(add(m, 0x40), x2)
            mstore(add(m, 0x60), y2)
            ok := staticcall(gas(), 6, m, 0x80, m, 0x40)
            x := mload(m)
            y := mload(add(m, 0x20))
        }
    }
}
