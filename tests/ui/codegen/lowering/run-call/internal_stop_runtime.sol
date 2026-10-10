//@ codegen-matrix: standard
//@ run-call: haltThenRevert

contract InternalStopRuntime {
    function haltThenRevert() external pure {
        halt();
        revert(); //~ WARN: unreachable code
    }

    function halt() internal pure {
        assembly {
            stop()
        }
    }
}
