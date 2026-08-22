//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: test() => 65, 99
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_array_packing_not_overlapping_variable.sol

contract StorageBoundaryArrayPacking {
    struct Canary {
        uint256 value;
    }

    function getArray() internal pure returns (uint64[10][1] storage array) {
        assembly {
            array.slot := sub(0, 1)
        }
    }

    function getCanary() internal pure returns (Canary storage canary) {
        assembly {
            canary.slot := 2
        }
    }

    function test() public returns (uint256 sum, uint256 sentinel) {
        Canary storage canary = getCanary();
        canary.value = 99;
        uint64[10][1] storage array = getArray();
        array[0] = [uint64(1), 2, 3, 4, 5, 6, 7, 8, 9, 10];
        array[0] = [uint64(11), 12, 13, 14, 15];
        for (uint256 i = 0; i < array[0].length; ++i) sum += array[0][i];
        sentinel = canary.value;
    }
}
