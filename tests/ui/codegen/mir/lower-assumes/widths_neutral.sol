//@ revisions: gas size
//@[gas] compile-flags: -Ogas --emit=bin-runtime
//@[size] compile-flags: -Osize --emit=bin-runtime

// Recording the facts of `widths.sol` never changes the code the compiler emits
// without `-Zcheck-analyses`: the e-graph and late constant folding rewrite the
// same way and record nothing, and so do check elimination's reduced branch
// conditions and fused tests. This bytecode is what the compiler emitted before
// those passes recorded anything, so a change to the recording that moves codegen
// shows here.
contract WidthsNeutral {
    function divided(uint8 a) external pure returns (uint256) {
        return uint256(a) / 300;
    }

    function masked(uint8 a) external pure returns (uint256) {
        return uint256(a) & 0xffff;
    }

    function power(uint8 e) external pure returns (uint256) {
        return 2 ** e;
    }

    function bounded_power(uint256 e) external pure returns (uint256) {
        require(e < 100);
        return 3 ** e;
    }

    function product(uint256 n) external pure returns (uint256 p) {
        p = 1;
        for (uint256 i = 2; i <= n; i++) {
            p *= i;
        }
    }

    function both(bool a, bool b) external pure returns (bool) {
        return a && b;
    }

    function either(bool a, bool b) external pure returns (bool) {
        return a != b;
    }

    function narrowed(uint8 a, uint8 b) external pure returns (bool) {
        return (uint256(a) << 8) == (uint256(b) << 8);
    }
}
