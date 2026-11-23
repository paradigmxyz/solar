//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ run-call: f 1 => 7
//@ run-call: f 2 => 7
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` the two equal returns stay separate blocks and the branch that chooses between them
// stays, so each `return` keeps its own location and a coverage run counts the branch.
contract C {
    // CHECK-LABEL: ; function f(
    // CHECK: eq
    // CHECK: push [[TAKEN:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: push 7 !metadata(span={{[0-9]+}}..{{[0-9]+}})
    // CHECK: return !metadata(span={{[0-9]+}}..{{[0-9]+}}, exit=return)
    // CHECK: [[TAKEN]] !metadata(span={{[0-9]+}}..{{[0-9]+}}):
    // CHECK-NEXT: push 7 !metadata(span={{[0-9]+}}..{{[0-9]+}})
    // CHECK: return !metadata(span={{[0-9]+}}..{{[0-9]+}}, exit=return)
    function f(uint256 x) external pure returns (uint256) {
        if (x == 1) {
            return 7;
        }
        return 7;
    }
}
