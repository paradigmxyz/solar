//@ compile-flags: --emit=bin-runtime --evm-version osaka -Zemit-lean-stages=../../target/ui-lean-stages
// `calldatacopy` and `mcopy` write memory in the semantics, so their stages export.

contract LeanStagesCopy {
    function echo(bytes calldata b) external pure returns (bytes memory) {
        return b;
    }

    function twice(bytes memory b) external pure returns (bytes memory) {
        return bytes.concat(b, b);
    }
}
