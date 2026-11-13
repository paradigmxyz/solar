//@ compile-flags: -Ogas -Zprove-analyses=- --emit=bin-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `-Zprove-analyses=-` writes each `@custom:ensures` clause as a query about the
// optimized program: the clause, implied by the `@custom:requires` premises,
// must hold where the function returns. The checked addition's overflow branch
// dominates the return, so the query also knows `x + 1` did not wrap.
contract SpecQueries {
    // CHECK-LABEL: ; query: SpecQueries-f0-{{.+}}-spec_ensures-0.smt2
    // CHECK: ; claim: spec_ensures [[GOAL:v[0-9]+]] ne 0x0
    // CHECK: (assert (= [[X:v[0-9]+]] (input_calldataload #x{{0+}}4)))
    // CHECK: (assert (= [[PRE:v[0-9]+]] (ite (bvult [[X]] #x{{0+}}64) #x{{0+}}1 #x{{0+}})))
    // CHECK: (assert (= [[R:v[0-9]+]] (bvadd [[X]] #x{{0+}}1)))
    // CHECK: (assert (= [[CLAIM:v[0-9]+]] (ite (bvult [[X]] [[R]]) #x{{0+}}1 #x{{0+}})))
    // CHECK: (assert (= [[UNMET:v[0-9]+]] (ite (= [[PRE]] #x{{0+}}) #x{{0+}}1 #x{{0+}})))
    // CHECK: (assert (= [[GOAL]] (bvor [[UNMET]] [[CLAIM]])))
    // CHECK: (assert (not (not (= [[GOAL]] #x{{0+}}))))
    // CHECK-NEXT: (check-sat)
    /// @custom:requires x < 100
    /// @custom:ensures r > x
    function next(uint256 x) external pure returns (uint256 r) {
        r = x + 1;
    }
}
