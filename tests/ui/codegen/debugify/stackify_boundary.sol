//@ compile-flags: --emit=bin -Onone -Zdebugify=report
//@ normalize-stdout-test: "(?s).+" -> ""
// The MIR carries the origins of both `calldataload` operands, but stack scheduling emits EVM IR
// without them. The `stackify` record reports that loss at the MIR to EVM IR boundary.
contract StackifyBoundary {
    function f() external pure {
        assembly {
            mstore(0, add(calldataload(4), calldataload(36)))
        }
    }
}
