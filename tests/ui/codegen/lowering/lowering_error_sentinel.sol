//@compile-flags: -Zcodegen -O none -Zdump=mir

// Unsupported constructs reported during lowering produce an error sentinel
// value instead of panicking or silently lowering to zero. This used to ICE.

contract RuntimeCodeTarget {}

contract LoweringErrorSentinel {
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length; //~ ERROR: codegen does not support `type(C).runtimeCode` yet
    }
}
