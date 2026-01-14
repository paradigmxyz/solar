//@compile-flags: -Ztypeck

contract FunctionTypeTests {
    function externalPure(uint256 x) external pure returns (uint256) { return x; }
    function externalView(uint256 x) external view returns (uint256) { return x; }

    // Valid: function pointer with same signature.
    function testFunctionPointerConversion() public view {
        function(uint256) external pure returns (uint256) f1 = this.externalPure;
        function(uint256) external view returns (uint256) f2 = this.externalView;
    }
}

contract FunctionTypeErrors {
    function extFn(uint256 x) external pure returns (uint256) { return x; }
    function extFnDiffParams(uint256 x, uint256 y) external pure returns (uint256) { return x + y; }
    function extFnDiffReturn(uint256 x) external pure returns (int256) { return int256(x); }

    // Valid: same signature assignment.
    function testValidAssignment() public view {
        function(uint256) external pure returns (uint256) f1 = this.extFn;
    }
}
