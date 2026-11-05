//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir -Zdebugify=check
//~? ERROR: debugify: MIR pass `lower-abi` left 6 instructions without a location
// A MIR dump without code generation runs the same pipeline, so the audit must see the gaps that
// `--emit=bin` reports.
contract MirOnly {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
