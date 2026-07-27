//@ run-call: callArray 10, 0 => 11
//@ run-call: callArray 10, 1 => 12
//@ run-call: callArray 10, 2 => 13
//@ run-call: callArray 10, 3 => 15
//@ run-call: callArray 10, 4 => 18
//@ run-call-fail: callArray 10, 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051

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
