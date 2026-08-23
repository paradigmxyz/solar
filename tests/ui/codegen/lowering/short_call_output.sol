//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: ShortCallOutput::shortOutput() => true, 7

// EVM calls only copy the bytes that the callee returned. Bytes after that
// remain unchanged, even when the requested output area is larger.
contract ShortCallOutput {
    function shortOutput() external returns (bool success, uint256 word) {
        assembly {
            mstore(0, 7)
            // Invalid input makes ecrecover return no bytes successfully.
            success := call(sub(0, 1), 1, 0, 0, 0, 0, 32)
            word := mload(0)
        }
    }
}
