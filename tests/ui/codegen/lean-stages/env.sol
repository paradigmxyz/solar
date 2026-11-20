//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// Block and transaction reads are the semantics' oracle, so their stages export.

contract LeanStagesEnv {
    function f() external view returns (uint256, uint256, uint256, address, uint256) {
        return (block.number, block.timestamp, block.chainid, tx.origin, address(this).balance);
    }
}
