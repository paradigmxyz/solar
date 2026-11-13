//@ compile-flags: -Ogas -Zprove-analyses=- --emit=bin-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `-Zprove-analyses=-` writes each fact a pass relied on as an SMT-LIB query on
// stdout. Storage is an array of words threaded through the function, so the
// read CSE forwarded from `total` is the stored sum. Calldata is a function of
// its offset, so repeated reads agree.
contract Queries {
    uint256 public total;

    // CHECK-LABEL: ; query: Queries-f1-bb2-v2-cse-1.smt2
    // CHECK: ; claim: cse v2 eq v3 at the end of bb2
    // CHECK: (assert (= v3 (bvadd v0 #x{{0+}}1)))
    // CHECK: (assert (= [[STORED:sto[0-9]+]] (store sto0 #x{{0+}} v3)))
    // CHECK: (assert (= v2 (select [[STORED]] #x{{0+}})))
    // CHECK: (assert (not (= v2 v3)))
    // CHECK-NEXT: (check-sat)
    function bump() external returns (uint256) {
        total = total + 1;
        return total;
    }

    // CHECK-LABEL: ; query: Queries-f2-bb1-v3-check_elim-0.smt2
    // CHECK: ; axioms: {{$}}
    // CHECK: (declare-fun input_calldataload ((_ BitVec 256)) (_ BitVec 256))
    // CHECK: (assert (= [[X:v[0-9]+]] (input_calldataload #x{{0+}}4)))
    // CHECK: (assert (= v1 (bvadd [[X]] #x{{0+}}1)))
    function next(uint256 x) external pure returns (uint256) {
        if (x < 10) {
            return x + 1;
        }
        return 0;
    }
}
