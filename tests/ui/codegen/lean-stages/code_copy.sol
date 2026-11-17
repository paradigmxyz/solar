//@ revisions: plain lean
//@ compile-flags: --emit=bin-runtime -Ogas --evm-version osaka
//@[lean] compile-flags: -Zemit-lean-stages=../../target/ui-lean-stages
// Returning another contract's creation code copies program data after the code: MIR's
// `datasize` and `datacopy` and EVM IR's pushes of its offset and size before `CODECOPY`, which
// the stages take from the assembled code and the manifest lists. Both revisions print the same
// bytecode: exporting leaves it as it is.

contract Child {}

contract LeanStagesCodeCopy {
    function code() external pure returns (bytes memory) {
        return type(Child).creationCode;
    }
}
