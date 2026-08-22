//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: f 42, 23 => 42, 23, 42, 23
//@[gas] run-call: f 42, 23 => 42, 23, 42, 23
//@[size] run-call: f 42, 23 => 42, 23, 42, 23
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
