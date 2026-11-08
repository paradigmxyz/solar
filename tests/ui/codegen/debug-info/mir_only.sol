//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~? ERROR: debugify: MIR module arrived with 3 instructions without a location
// A MIR dump without code generation runs the same pipeline, so the audit must see the gaps that
// `--emit=bin` reports. The constructor that stages a library's deployment address has no source
// construct, so its instructions come out without an origin.
library MirOnly {
    event Done(uint256 x);

    function f(uint256 x) external returns (uint256) {
        emit Done(x);
        return x + 1;
    }
}
