//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: use(bytes) 0x010203 => 1
//@ run-call: use(bytes) 0x01 => 1

contract CalldataSliceRecursive {
    function peel(bytes calldata data)
        internal
        pure
        returns (bytes calldata)
    {
        if (data.length < 2) return data;
        return peel(data[1:]);
    }

    function use(bytes calldata data) external pure returns (uint256) {
        return peel(data).length;
    }
}
