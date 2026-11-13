//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// `keccak256` is the semantics' hash oracle in MIR and EVM IR, so its stages export.

contract LeanStagesKeccak {
    function f(bytes32 a) external pure returns (bytes32) {
        return keccak256(abi.encode(a));
    }
}
