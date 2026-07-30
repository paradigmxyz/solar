//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/array/function_memory_array.sol

contract FunctionPointerMemoryArray {
    function arrayA(uint256 x) public pure returns (uint256) {
        return x + 1;
    }

    function arrayB(uint256 x) public pure returns (uint256) {
        return x + 2;
    }

    function arrayC(uint256 x) public pure returns (uint256) {
        return x + 3;
    }

    function arrayD(uint256 x) public pure returns (uint256) {
        return x + 5;
    }

    function arrayE(uint256 x) public pure returns (uint256) {
        return x + 8;
    }

    // CHECK-LABEL: fn @callArray(
    // CHECK: mstore {{v[0-9]+}}, [[A:[0-9]+]]
    // CHECK: mstore {{v[0-9]+}}, [[B:[0-9]+]]
    // CHECK: mstore {{v[0-9]+}}, [[C:[0-9]+]]
    // CHECK: mstore {{v[0-9]+}}, [[D:[0-9]+]]
    // CHECK: mstore {{v[0-9]+}}, [[E:[0-9]+]]
    // CHECK: [[ARRAY_FN:v[0-9]+]] = mload {{v[0-9]+}}
    // CHECK: internal_call @__internal_dispatch_0, 1, [[ARRAY_FN]], arg0
    // CHECK-LABEL: fn @__internal_dispatch_0(
    // CHECK: eq arg0, [[A]]
    // CHECK: internal_call @arrayA.7, 1, arg1
    // CHECK: eq arg0, [[B]]
    // CHECK: internal_call @arrayB.8, 1, arg1
    // CHECK: eq arg0, [[C]]
    // CHECK: internal_call @arrayC.9, 1, arg1
    // CHECK: eq arg0, [[D]]
    // CHECK: internal_call @arrayD.10, 1, arg1
    // CHECK: eq arg0, [[E]]
    // CHECK: internal_call @arrayE.11, 1, arg1
    function callArray(uint256 x, uint256 index) public returns (uint256) {
        function(uint256) internal returns (uint256)[] memory functions =
            new function(uint256) internal returns (uint256)[](10);
        functions[0] = arrayA;
        functions[1] = arrayB;
        functions[2] = arrayC;
        functions[3] = arrayD;
        functions[4] = arrayE;
        return functions[index](x);
    }
}
