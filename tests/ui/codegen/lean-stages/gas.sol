//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// `gasleft()` reads the semantics' oracle at a history the read extends, so its stages export.

contract LeanStagesGas {
    function f() external view returns (uint256) {
        return gasleft();
    }
}
