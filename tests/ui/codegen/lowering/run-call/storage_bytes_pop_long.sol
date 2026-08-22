//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: test => true
// ported-from: test/libsolidity/semanticTests/array/pop/byte_array_pop_long_storage_empty.sol

contract StorageBytesPopLong {
    bytes data;

    function test() external returns (bool) {
        for (uint256 i; i <= 40; ++i) data.push(bytes1(uint8(i + 1)));
        for (int256 j = 40; j >= 0; --j) {
            require(data[uint256(j)] == bytes1(uint8(uint256(j) + 1)));
            require(data.length == uint256(j + 1));
            data.pop();
        }
        return true;
    }
}
