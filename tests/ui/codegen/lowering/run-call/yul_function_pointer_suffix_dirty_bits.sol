//@ run-call: suffixes => 0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff, 0x00000000000000000000000000000000000000000000000000000000ffffffff

// Assigning `.address` and `.selector` in assembly drops bits outside each
// field, where solc keeps the raw words; see CODEGEN-012 in
// docs/SOLC_DIVERGENCE.md.

contract YulFunctionPointerSuffixDirtyBits {
    function target() external {}

    function suffixes() external view returns (uint256 a, uint256 s) {
        function() external f = this.target;
        assembly {
            function dirty() -> x, y {
                x := not(0)
                y := not(0)
            }
            f.address, f.selector := dirty()
            a := f.address
            s := f.selector
        }
    }
}
