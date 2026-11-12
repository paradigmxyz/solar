//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ compile-flags: --emit=bin -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""

// One materialization of P is retained for several consumers, so it takes their merged location
// instead of leaving an unexplained gap.
contract RetainedConstant {
    uint256 constant P = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff;

    function f(uint256 x, uint256 y) external pure returns (uint256) {
        return addmod(mulmod(x, y, P), mulmod(x, x, P), P);
    }
}
