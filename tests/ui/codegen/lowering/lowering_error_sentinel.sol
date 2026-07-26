//@compile-flags: -Zcodegen -Zdump=mir

// Unsupported constructs reported during lowering produce an error sentinel
// value instead of panicking or silently lowering to zero. This used to ICE.

contract LoweringErrorSentinel {
    function runtime() external pure returns (uint256) {
        return type(LoweringErrorSentinel).runtimeCode.length; //~ ERROR: codegen does not support `type(C).runtimeCode` yet
    }

    function callFunctionValue(uint256 value) external pure returns (uint256) {
        function(uint256) pure returns (uint256) f = double;
        return f(value); //~ ERROR: codegen does not support this call expression yet
    }

    function callUnitFunctionValue(uint256 value) external pure {
        function(uint256) pure f = consume;
        f(value); //~ ERROR: codegen does not support this call expression yet
    }

    function double(uint256 value) internal pure returns (uint256) {
        return value * 2;
    }

    function consume(uint256) internal pure {}
}
