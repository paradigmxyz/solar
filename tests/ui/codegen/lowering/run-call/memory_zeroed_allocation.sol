//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: readDynamic(uint256,uint256) 5, 0 => 0
//@ run-call: readDynamic(uint256,uint256) 5, 4 => 0
//@ run-call-fail: readDynamic(uint256,uint256) 5, 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readNested(uint256,uint256,uint256,uint256) 2, 4, 1, 3 => 0
//@ run-call-fail: readNested(uint256,uint256,uint256,uint256) 2, 4, 1, 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readFixed(uint256) 2 => 0
//@ run-call-fail: readFixed(uint256) 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readStatic(uint256,uint256) 2, 1 => 0
//@ run-call-fail: readStatic(uint256,uint256) 2, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// ported-from: test/libsolidity/semanticTests/array/arrayMemoryAllocation/array_zeroed_memory_index_access.sol
// ported-from: test/libsolidity/semanticTests/array/arrayMemoryAllocation/array_2d_zeroed_memory_index_access.sol
// ported-from: test/libsolidity/semanticTests/array/arrayMemoryAllocation/array_static_zeroed_memory_index_access.sol
// ported-from: test/libsolidity/semanticTests/array/arrayMemoryAllocation/array_array_static.sol

contract MemoryZeroedAllocation {
    function readDynamic(uint256 length, uint256 index) external pure returns (uint256) {
        uint256[] memory values = new uint256[](length);
        return values[index];
    }

    function readNested(uint256 outerLength, uint256 innerLength, uint256 outer, uint256 inner)
        external
        pure
        returns (uint256)
    {
        uint256[][] memory values = new uint256[][](outerLength);
        for (uint256 i; i < outerLength; ++i) {
            values[i] = new uint256[](innerLength);
        }
        return values[outer][inner];
    }

    function readFixed(uint256 index) external pure returns (uint256) {
        uint256[3] memory values;
        return values[index];
    }

    function readStatic(uint256 length, uint256 index) external pure returns (uint256) {
        uint256[4][] memory values = new uint256[4][](length);
        return values[index][0];
    }
}
