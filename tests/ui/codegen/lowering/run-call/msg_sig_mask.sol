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
//@[none] run-call: 0xb3de648b0000000100000000000000000000000000000000000000000000000000000000 => 0xb3de648b00000000000000000000000000000000000000000000000000000000
//@[gas] run-call: 0xb3de648b0000000100000000000000000000000000000000000000000000000000000000 => 0xb3de648b00000000000000000000000000000000000000000000000000000000
//@[size] run-call: 0xb3de648b0000000100000000000000000000000000000000000000000000000000000000 => 0xb3de648b00000000000000000000000000000000000000000000000000000000

contract MsgSigMask {
    function f(uint256 value) external pure returns (bytes32) {
        return bytes32(msg.sig);
    }
}
