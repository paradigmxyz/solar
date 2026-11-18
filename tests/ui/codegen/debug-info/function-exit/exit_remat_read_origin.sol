//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ compile-flags: --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A function that falls off the end of its body evaluates no expression at its exit: it only
// returns the named return variables. A read that the optimizer recomputes in that return encoding
// carries the value of the statement that read it, so it keeps the origin of that statement, as
// LLVM's rematerialization keeps the location of the original. The read of `block.number` is the
// only instruction of its statement. The exit itself maps to the closing brace.
contract C {
    // CHECK-LABEL: ; function block_number(
    // CHECK: number !metadata(span=[[#READ:]]..
    // CHECK-SAME: [[#READ+12]])
    // CHECK: return !metadata(span=[[#END:]]..
    // CHECK-SAME: [[#END+1]], exit=return)
    function block_number() external view returns (uint256 blockNumber) {
        blockNumber = block.number;
    }
}
