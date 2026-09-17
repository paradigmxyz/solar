//@ codegen-matrix: standard
//@ run-call: haltThenRevert

contract InternalStopRuntime {
    function haltThenRevert() external pure {
        halt();
        revert();
    }

    function halt() internal pure {
        assembly {
            stop()
        }
    }
}
