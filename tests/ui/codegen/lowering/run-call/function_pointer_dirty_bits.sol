//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: comparison() => true
//@[none, gas, size] run-call: abiEncoding() => true
// ported-from: test/libsolidity/semanticTests/functionTypes/comparison_operator_for_external_function_cleans_dirty_bits.sol

contract FunctionPointerDirtyBits {
    function g() external {}

    function comparison() external view returns (bool) {
        function() external g_ptr_dirty = this.g;
        assembly {
            g_ptr_dirty.address := or(g_ptr_dirty.address, shl(160, sub(0, 1)))
            g_ptr_dirty.selector := or(g_ptr_dirty.selector, shl(32, sub(0, 1)))
        }
        function() external g_ptr = this.g;
        return g_ptr == g_ptr_dirty;
    }

    function abiEncoding() external view returns (bool) {
        function() external g_ptr_dirty = this.g;
        assembly {
            g_ptr_dirty.address := or(g_ptr_dirty.address, shl(160, sub(0, 1)))
            g_ptr_dirty.selector := or(g_ptr_dirty.selector, shl(32, sub(0, 1)))
        }
        return keccak256(abi.encode(g_ptr_dirty)) == keccak256(abi.encode(this.g));
    }
}
