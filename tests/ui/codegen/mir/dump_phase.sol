//@ revisions: none gas pipeline substitute evm-substitute
//@[none] compile-flags: -Zcodegen -O none -Zdump=mir
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -Zcodegen -O gas -Zdump=mir
//@[gas] filecheck: --check-prefix=GAS
//@[pipeline] compile-flags: -Zmir-pipeline=none
//@[pipeline] filecheck: --check-prefix=PIPELINE
//@[substitute] compile-flags: -Zcodegen -O gas -Zdump=mir -Zmir-pipeline=none
//@[substitute] filecheck: --check-prefix=SUBSTITUTE
//@[evm-substitute] compile-flags: -Zcodegen -Zdump=evm-ir-runtime -Zevm-ir-pipeline=none
//@[evm-substitute] filecheck: --check-prefix=EVM-SUBSTITUTE

// NONE: @module DumpPhase
// NONE-NOT: @phase
// NONE-LABEL: fn @f(arg0: u256)
// NONE: add arg0, 0
// GAS: @phase evm-shaped
// GAS-NOT: add arg0, 0
// PIPELINE: {{^// === .*:DumpPhase \(after none\) ===$}}
// PIPELINE: @module DumpPhase
// PIPELINE: add arg0, 0
// SUBSTITUTE: @module DumpPhase
// SUBSTITUTE-NOT: @phase
// SUBSTITUTE: add arg0, 0
// EVM-SUBSTITUTE: @module runtime
// EVM-SUBSTITUTE: jump [[NEXT:bb[0-9]+]]
// EVM-SUBSTITUTE-NEXT: [[NEXT]]:
contract DumpPhase {
    function f(uint256 x) public pure returns (uint256) {
        return x + 0;
    }
}
