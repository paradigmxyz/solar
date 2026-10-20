//@ codegen-matrix: standard
//@ run-call: far() => 0xbeef
contract Test {
    // A far word is written, then a buffer is taken from `msize()`, and the far word is written
    // again. The first far store is dead but places the buffer above it.
    function far() external pure returns (uint256 r) {
        assembly {
            mstore(0x2100, 1)
            let q := msize()
            mstore(add(q, 0x2080), 0xbeef)
            mstore(0x2100, 2)
            r := mload(add(q, 0x2080))
        }
    }
}
