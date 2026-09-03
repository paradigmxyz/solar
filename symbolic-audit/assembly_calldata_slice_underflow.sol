// A calldata slice whose length underflows in assembly, then copied to memory
// for the ABI-encoded return. Called with an empty `bytes`.
// solc reverts with Panic(0x41); solar reverts with empty returndata.
// Source: tests/ui/codegen/lowering/run-call/forwarded_calldata_slice_return.sol
contract AssemblyCalldataSliceUnderflow {
    function delegate(bytes calldata executionData) external pure returns (address, bytes memory) {
        return _delegate(executionData);
    }

    function _delegate(bytes calldata executionData)
        internal
        pure
        returns (address target, bytes calldata data)
    {
        assembly {
            target := shr(96, calldataload(executionData.offset))
            data.offset := add(executionData.offset, 20)
            data.length := sub(executionData.length, 20)
        }
    }
}
