//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: 0xb3de648b0000000100000000000000000000000000000000000000000000000000000000 => 0xb3de648b00000000000000000000000000000000000000000000000000000000

contract MsgSigMask {
    function f(uint256 value) external pure returns (bytes32) {
        return bytes32(msg.sig);
    }
}
