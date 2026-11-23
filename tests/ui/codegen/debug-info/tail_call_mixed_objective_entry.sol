//@ compile-flags: -Onone -Zdump=evm-ir-runtime --emit=srcmap-runtime,ethdebug-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized function whose only code is a call to a function tagged for another objective
// that never returns falls through into it, and both entries share the first instruction. The
// assembler marks one and does not panic. The jump into the callee stays, whatever the callee's
// objective.
contract C {
    // CHECK-LABEL: invoke=outer@{{[0-9]+}}
    // CHECK-NEXT: jump [[CALLEE:bb[0-9]+]]
    // CHECK: [[CALLEE]] [{{.*}}function_entry{{.*}}:
    function outer() external pure {
        inner();
    }

    /// @custom:solar-optimize gas
    function inner() internal pure {
        assembly {
            stop()
        }
    }
}
