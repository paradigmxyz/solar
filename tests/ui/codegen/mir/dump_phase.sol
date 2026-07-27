//@ revisions: none gas
//@[none] compile-flags: -Zcodegen -O none -Zdump=mir
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -Zcodegen -O gas -Zdump=mir
//@[gas] filecheck: --check-prefix=GAS

// NONE: @module DumpPhase
// NONE-NOT: @phase
// NONE-LABEL: fn @f(arg0: u256)
// NONE: add arg0, 0
// GAS: @phase evm-shaped
// GAS-NOT: add arg0, 0
contract DumpPhase {
    function f(uint256 x) public pure returns (uint256) {
        return x + 0;
    }
}
