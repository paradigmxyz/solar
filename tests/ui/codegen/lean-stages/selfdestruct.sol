//@ compile-flags: --emit=bin-runtime --evm-version osaka -Zemit-lean-stages=../../target/ui-lean-stages
// `selfdestruct` ends the frame as `stop` does and records its beneficiary in the world, so its
// stages export.

contract LeanStagesSelfdestruct {
    function kill(address payable to) external {
        selfdestruct(to);
    }
}
