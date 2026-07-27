//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

// ported-from: test/libsolidity/semanticTests/array/function_memory_array.sol

// OPT: @phase evm-shaped
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

    // BUILT-LABEL: fn @callArray(
    // BUILT: mstore {{v[0-9]+}}, [[A:[0-9]+]]
    // BUILT: mstore {{v[0-9]+}}, [[B:[0-9]+]]
    // BUILT: mstore {{v[0-9]+}}, [[C:[0-9]+]]
    // BUILT: mstore {{v[0-9]+}}, [[D:[0-9]+]]
    // BUILT: mstore {{v[0-9]+}}, [[E:[0-9]+]]
    // BUILT: [[ARRAY_FN:v[0-9]+]] = mload {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_0, 1, [[ARRAY_FN]], arg0
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[A]]
    // BUILT: internal_call arrayA{{[0-9]+}}, 1, arg1
    // BUILT: eq arg0, [[B]]
    // BUILT: internal_call arrayB{{[0-9]+}}, 1, arg1
    // BUILT: eq arg0, [[C]]
    // BUILT: internal_call arrayC{{[0-9]+}}, 1, arg1
    // BUILT: eq arg0, [[D]]
    // BUILT: internal_call arrayD{{[0-9]+}}, 1, arg1
    // BUILT: eq arg0, [[E]]
    // BUILT: internal_call arrayE{{[0-9]+}}, 1, arg1
    // OPT-LABEL: fn @callArray(
    // OPT: [[ARRAY_FN:v[0-9]+]] = mload {{v[0-9]+}}
    // OPT: eq [[ARRAY_FN]], 1
    // OPT: internal_call arrayA{{[0-9]+}}, 1, arg0
    // OPT: eq [[ARRAY_FN]], 2
    // OPT: internal_call arrayB{{[0-9]+}}, 1, arg0
    // OPT: eq [[ARRAY_FN]], 3
    // OPT: internal_call arrayC{{[0-9]+}}, 1, arg0
    // OPT: eq [[ARRAY_FN]], 4
    // OPT: internal_call arrayD{{[0-9]+}}, 1, arg0
    // OPT: eq [[ARRAY_FN]], 5
    // OPT: internal_call arrayE{{[0-9]+}}, 1, arg0
    // OPT: mstore 4, 81
    // OPT-NOT: fn @__internal_dispatch_0(
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
