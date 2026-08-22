//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: g false => 23, 37, 71
//@[gas] run-call: g false => 23, 37, 71
//@[size] run-call: g false => 23, 37, 71
//@[none] run-call: g true => 23, 37, 71
//@[gas] run-call: g true => 23, 37, 71
//@[size] run-call: g true => 23, 37, 71
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_function_types_v2.sol

contract ExternalFunctionPointerCalldataArray {
    function f(function() external returns (uint256)[] calldata pointers)
        external
        returns (uint256, uint256, uint256)
    {
        require(pointers.length == 3);
        return (pointers[0](), pointers[1](), pointers[2]());
    }

    function fReenc(function() external returns (uint256)[] calldata pointers)
        external
        returns (uint256, uint256, uint256)
    {
        return this.f(pointers);
    }

    function getter1() external pure returns (uint256) {
        return 23;
    }

    function getter2() external pure returns (uint256) {
        return 37;
    }

    function getter3() external pure returns (uint256) {
        return 71;
    }

    function g(bool reencode) external returns (uint256, uint256, uint256) {
        function() external returns (uint256)[] memory pointers =
            new function() external returns (uint256)[](3);
        pointers[0] = this.getter1;
        pointers[1] = this.getter2;
        pointers[2] = this.getter3;
        return reencode ? this.fReenc(pointers) : this.f(pointers);
    }
}
