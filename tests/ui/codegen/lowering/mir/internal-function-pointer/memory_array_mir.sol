//@ compile-flags: -O none -Zdump=mir
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
    // CHECK: [[ARR:v[0-9]+]] = alloc
    // CHECK: [[ARR_32:v[0-9]+]] = ptradd [[ARR]], 32
    // CHECK-NEXT: mstore [[ARR_32]], [[A:[0-9]+]]
    // CHECK: [[ARR_64:v[0-9]+]] = ptradd [[ARR]], 64
    // CHECK-NEXT: mstore [[ARR_64]], [[B:[0-9]+]]
    // CHECK: [[ARR_96:v[0-9]+]] = ptradd [[ARR]], 96
    // CHECK-NEXT: mstore [[ARR_96]], [[C:[0-9]+]]
    // CHECK: [[ARR_128:v[0-9]+]] = ptradd [[ARR]], 128
    // CHECK-NEXT: mstore [[ARR_128]], [[D:[0-9]+]]
    // CHECK: [[ARR_160:v[0-9]+]] = ptradd [[ARR]], 160
    // CHECK-NEXT: mstore [[ARR_160]], [[E:[0-9]+]]
    // CHECK: [[OFF1:v[0-9]+]] = shl arg1, 5
    // CHECK-NEXT: [[ELEM:v[0-9]+]] = ptradd [[ARR]], [[OFF1]]
    // CHECK-NEXT: [[ELEM_32:v[0-9]+]] = ptradd [[ELEM]], 32
    // CHECK-NEXT: [[ARRAY_FN:v[0-9]+]] = mload [[ELEM_32]]
    // CHECK: icall @internal_dispatcher{{.*}}, [[ARRAY_FN]], arg0
    // CHECK-LABEL: fn @internal_dispatcher{{.*}}(
    // CHECK: eq arg0, [[A]]
    // CHECK: eq arg0, [[B]]
    // CHECK: eq arg0, [[C]]
    // CHECK: eq arg0, [[D]]
    // CHECK: eq arg0, [[E]]
    // CHECK: icall @arrayE, arg1
    // CHECK: icall @arrayD, arg1
    // CHECK: icall @arrayC, arg1
    // CHECK: icall @arrayB, arg1
    // CHECK: icall @arrayA, arg1
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
