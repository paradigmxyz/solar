//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
// solc maps library deployment-address staging to the contract (ContractCompiler.cpp L231).
// Its setup and ABI prologue keep that origin, so both audit modes accept the whole pipeline.
library GapCheck {
    event Done(uint256 x);

    function f(uint256 x) external returns (uint256) {
        emit Done(x);
        return x + 1;
    }
}
