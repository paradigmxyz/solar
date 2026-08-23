//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: test() => 3
// ported-from: test/libsolidity/semanticTests/viaYul/return_storage_pointers.sol

contract StorageReturnPointerMulti {
    uint256[] arr1;
    uint256[][] arr2;

    function refs() internal view returns (uint256[] storage ptr1, uint256[][] storage ptr2) {
        ptr1 = arr1;
        ptr2 = arr2;
    }

    function test() public returns (uint256) {
        uint256[] storage ptr1;
        uint256[][] storage ptr2;
        (ptr1, ptr2) = refs();
        ptr1.push(7);
        ptr2.push();
        ptr2[0].push(9);
        return ptr1.length + ptr2.length + ptr2[0].length;
    }
}
