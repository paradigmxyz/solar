//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
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
