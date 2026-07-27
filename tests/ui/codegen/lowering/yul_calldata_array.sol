//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// `.length`/`.offset` on a calldata array in inline assembly. The array
// parameter's value is the ABI head, so `.length` reads the length word at
// `4 + head` and `.offset` is `4 + head + 32` (first element). Runtime-verified.
contract C {
    // CHECK-LABEL: fn @probe{{[( ]}}
    // CHECK: {{v[0-9]+}} = slice_len arg0
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr arg0
    // CHECK: {{v[0-9]+}} = calldataload [[PTR]]
    // CHECK: returndata 128, 64
    function probe(uint256[] calldata a) external pure returns (uint256 len, uint256 first) {
        assembly {
            len := a.length
            first := calldataload(a.offset)
        }
    }
}
