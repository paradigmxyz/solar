//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: array() => 0, 1, 7
//@[none, gas, size] run-call: bytesValue() => 0, 2, 0xabcd

contract DeleteMemoryReference {
    function array() external pure returns (uint256, uint256, uint256) {
        uint256[] memory a = new uint256[](1);
        a[0] = 7;
        uint256[] memory b = a;
        delete a;
        return (a.length, b.length, b[0]);
    }

    function bytesValue() external pure returns (uint256, uint256, bytes2) {
        bytes memory a = hex"abcd";
        bytes memory b = a;
        delete a;
        return (a.length, b.length, bytes2(b));
    }
}
