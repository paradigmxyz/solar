//@ codegen-matrix: standard
//@ run-call: ForwardedCalldataSliceReturn::delegate 0x0000000000000000000000000000000000001234112233445566 => 0x0000000000000000000000000000000000001234, 0x112233445566
//@ run-call: ForwardedCalldataSliceReturn::single 0x0000000000000000000000000000000000001234112233445566 => 0x0000000000000000000000000000000000001234, 7, 0x112233445566
//@ run-call: ForwardedCalldataSliceReturn::nested 0x0000000000000000000000000000000000001234112233445566 => 0x0000000000000000000000000000000000001234, 7, 0x112233445566
//@ run-call: ForwardedCalldataSliceReturn::nestedSlices 0x112233445566778899aabbccddeeff0011223344 => 0x11, 0x2233445566778899aabbccddeeff001122334400
//@ run-call: ForwardedCalldataSliceReturn::singleSliceLength 0x1122 => 2
//@ run-call: ForwardedCalldataSliceReturn::pointerSliceLength 0x1122 => 2
//@ run-call: ForwardedCalldataSliceReturn::arraySliceLength [7, 8] => 2
//@ run-call-fail: ForwardedCalldataSliceReturn::delegate 0x => Panic(0x41)

contract ForwardedCalldataSliceReturn {
    function singleSliceLength(bytes calldata executionData) external pure returns (uint256) {
        return _identity(executionData).length;
    }

    function pointerSliceLength(bytes calldata executionData) external pure returns (uint256) {
        function(bytes calldata) internal pure returns (bytes calldata) target = _identity;
        return target(executionData).length;
    }

    function arraySliceLength(uint256[] calldata values) external pure returns (uint256) {
        return _identityArray(values).length;
    }

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

    function nested(bytes calldata executionData)
        external
        pure
        returns (address, uint256, bytes memory)
    {
        return _nested(executionData);
    }

    function _nested(bytes calldata executionData)
        internal
        pure
        returns (address, uint256, bytes calldata)
    {
        return _single(executionData);
    }

    function _identity(bytes calldata executionData)
        internal
        pure
        returns (bytes calldata)
    {
        return executionData;
    }

    function _identityArray(uint256[] calldata values)
        internal
        pure
        returns (uint256[] calldata)
    {
        return values;
    }

    function nestedSlices(bytes calldata executionData)
        external
        pure
        returns (uint256, address)
    {
        return _parseSlices(executionData);
    }

    function _parseSlices(bytes calldata executionData)
        internal
        pure
        returns (uint256 value, address target)
    {
        (bool success, uint256 value_, address target_) = _tryParseSlices(executionData);
        require(success);
        return (value_, target_);
    }

    function _tryParseSlices(bytes calldata executionData)
        internal
        pure
        returns (bool success, uint256 value, address target)
    {
        (bool success_,, bytes memory first, bytes memory rest) = _splitSlices(executionData);
        success = success_;
        value = uint8(first[0]);
        assembly {
            target := shr(96, mload(add(rest, 0x20)))
        }
    }

    function _splitSlices(bytes calldata executionData)
        internal
        pure
        returns (bool, bytes2, bytes memory, bytes memory)
    {
        return (true, 0, executionData[:1], executionData[1:]);
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
