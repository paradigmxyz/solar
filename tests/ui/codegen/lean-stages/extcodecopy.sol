//@ compile-flags: --emit=bin-runtime --evm-version osaka -Zemit-lean-stages=../../target/ui-lean-stages
// `extcodecopy` writes memory with the code the oracle gives the account, so its stages export.

contract LeanStagesExtcodecopy {
    function code(address a) external view returns (bytes memory) {
        return a.code;
    }
}
