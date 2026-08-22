//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: testViewToDefault => 12, 22
//@ run-call: testPureToDefault => 13, 23
//@ run-call: testPureToView => 13, 23
// ported-from: test/libsolidity/semanticTests/conversions/function_type_array_to_storage.sol

contract FunctionPointerMutabilityConversion {
    function() external returns (uint256)[1] externalDefault;
    function() external view returns (uint256)[1] externalView;
    function() external pure returns (uint256)[1] externalPure;

    function() internal returns (uint256)[1] internalDefault;
    function() internal view returns (uint256)[1] internalView;
    function() internal pure returns (uint256)[1] internalPure;

    function externalDefaultTarget() external pure returns (uint256) {
        return 11;
    }

    function externalViewTarget() external pure returns (uint256) {
        return 12;
    }

    function externalPureTarget() external pure returns (uint256) {
        return 13;
    }

    function internalDefaultTarget() internal pure returns (uint256) {
        return 21;
    }

    function internalViewTarget() internal pure returns (uint256) {
        return 22;
    }

    function internalPureTarget() internal pure returns (uint256) {
        return 23;
    }

    function testViewToDefault() external returns (uint256, uint256) {
        externalDefault = [this.externalViewTarget];
        internalDefault = [internalViewTarget];
        return (externalDefault[0](), internalDefault[0]());
    }

    function testPureToDefault() external returns (uint256, uint256) {
        externalDefault = [this.externalPureTarget];
        internalDefault = [internalPureTarget];
        return (externalDefault[0](), internalDefault[0]());
    }

    function testPureToView() external returns (uint256, uint256) {
        externalView = [this.externalPureTarget];
        internalView = [internalPureTarget];
        return (externalView[0](), internalView[0]());
    }
}
