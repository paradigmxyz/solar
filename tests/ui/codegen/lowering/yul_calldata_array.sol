//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// `.length`/`.offset` on a calldata array in inline assembly. The array
// parameter's value is the ABI head, so `.length` reads the length word at
// `4 + head` and `.offset` is `4 + head + 32` (first element). Runtime-verified.
contract C {
    function probe(uint256[] calldata a) external pure returns (uint256 len, uint256 first) {
        assembly {
            len := a.length
            first := calldataload(a.offset)
        }
    }
}
