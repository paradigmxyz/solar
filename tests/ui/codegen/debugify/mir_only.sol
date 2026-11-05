//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~? ERROR: debugify: MIR pass `lower-abi` left 15 instructions without a location
// A MIR dump without code generation runs the same pipeline, so the audit must see the gaps that
// `--emit=bin` reports. The decoder that both `bytes` parameters share has no source construct.
contract MirOnly {
    function f(bytes calldata a, bytes calldata b) external pure returns (uint256) {
        return a.length + b.length;
    }
}
