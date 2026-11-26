//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages-ns -Zlean-stages-namespace=Run.1x
// The exporter refuses a namespace that is not dot-separated Lean identifiers.

contract LeanStagesNamespace {
    function f() external pure returns (uint256) {
        return 1;
    }
}

//~? ERROR: -Zemit-lean-stages: `-Zlean-stages-namespace` must be dot-separated identifiers, not `Run.1x`
