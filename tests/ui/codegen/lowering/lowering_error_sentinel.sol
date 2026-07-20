//@compile-flags: -Zcodegen -Zdump=mir
//@check-fail

// Unsupported constructs reported during lowering produce an error sentinel
// value instead of panicking or silently lowering to zero. This used to ICE.

contract LoweringErrorSentinel {
    function runtime() external pure returns (uint256) {
        return type(LoweringErrorSentinel).runtimeCode.length; //~ ERROR: codegen does not support `type(C).runtimeCode` yet
    }
}
