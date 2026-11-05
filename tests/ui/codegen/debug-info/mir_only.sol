//@ compile-flags: -Zmir-pipeline=lower-abi,lower-dispatch -Zdump=mir -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~? ERROR: debugify: MIR pass `lower-dispatch` left 8 instructions without a location
// A MIR dump without code generation runs the same pipeline, so the audit must see the gaps that
// `--emit=bin` reports. The selector dispatch that `lower-dispatch` builds has no source
// construct, so its instructions come out without an origin.
contract MirOnly {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
