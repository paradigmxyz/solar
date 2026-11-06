//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
// A MIR-only audit also preserves the library contract origin when ABI lowering creates the
// deployment-address staging constructor's nonpayable check.
library MirOnly {
    event Done(uint256 x);

    function f(uint256 x) external returns (uint256) {
        emit Done(x);
        return x + 1;
    }
}
