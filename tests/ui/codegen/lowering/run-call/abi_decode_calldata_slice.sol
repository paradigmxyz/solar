//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f 42, 23 => 42, 23, 42, 23
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_slice.sol

contract AbiDecodeCalldataSlice {
    function f(uint256 a, uint256 b)
        external
        pure
        returns (uint256 c, uint256 d, uint256 e, uint256 f_)
    {
        (c, d) = abi.decode(msg.data[4:], (uint256, uint256));
        e = abi.decode(msg.data[4 : 4 + 32], (uint256));
        f_ = abi.decode(msg.data[4 + 32 : 4 + 32 + 32], (uint256));
    }
}
