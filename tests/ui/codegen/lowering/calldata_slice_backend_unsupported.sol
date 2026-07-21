//@compile-flags: -Zcodegen --emit=bin-runtime
//@check-fail

// A calldata slice whose aggregate use slice lowering cannot fold — here two
// calldata slices built in assembly and passed on to an internal call — is
// reported as an unsupported construct rather than reaching the word-based
// backend, which cannot emit a logical slice. The check keeps such a slice from
// panicking the emitter.
contract CalldataSliceBackendUnsupported { //~ ERROR: codegen does not support this calldata-slice usage yet
    struct Call {
        address to;
        uint256 value;
        bytes data;
    }

    function _execute(Call[] calldata calls, bytes calldata opData)
        internal
        pure
        returns (uint256)
    {
        return calls.length + opData.length;
    }

    function execute(bytes calldata executionData) external pure returns (uint256) {
        Call[] calldata calls;
        bytes calldata opData;
        assembly {
            opData.length := 0
            let o := add(executionData.offset, calldataload(executionData.offset))
            calls.offset := add(o, 0x20)
            calls.length := calldataload(o)
        }
        return _execute(calls, opData);
    }
}
