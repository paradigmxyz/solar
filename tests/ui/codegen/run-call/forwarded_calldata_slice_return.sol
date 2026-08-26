//@ run-call: ForwardedCalldataSliceReturn::delegate(bytes) 0x0000000000000000000000000000000000001234112233445566 => 0x0000000000000000000000000000000000001234, 0x112233445566
//@ run-call: ForwardedCalldataSliceReturn::single(bytes) 0x0000000000000000000000000000000000001234112233445566 => 0x0000000000000000000000000000000000001234, 7, 0x112233445566

contract ForwardedCalldataSliceReturn {
    function delegate(bytes calldata executionData)
        external
        pure
        returns (address, bytes memory)
    {
        return _delegate(executionData);
    }

    function single(bytes calldata executionData)
        external
        pure
        returns (address, uint256, bytes memory)
    {
        return _single(executionData);
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

    function _single(bytes calldata executionData)
        internal
        pure
        returns (address target, uint256 value, bytes calldata data)
    {
        assembly {
            target := shr(96, calldataload(executionData.offset))
            value := 7
            data.offset := add(executionData.offset, 20)
            data.length := sub(executionData.length, 20)
        }
    }
}
