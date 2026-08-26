//@ run-call-fail: CalldataStructMemberSliceBounds::edge((bytes,bytes)) (0x0000000000000000000000000000000000000000000000000000000000000040, 0x0000000000000000000000000000000000000000000000000000000000000000) => 0xba597e7e
//@ run-call-fail: CalldataStructMemberSliceBounds::abiEdge((bytes,bytes)) (0x0000000000000000000000000000000000000000000000000000000000000040, 0x0000000000000000000000000000000000000000000000000000000000000000)

contract CalldataStructMemberSliceBounds {
    error DecodingError();

    struct S {
        bytes executionData;
        bytes garbage;
    }

    struct Call {
        address target;
        uint256 value;
        bytes data;
    }

    function edge(S calldata s) external pure returns (uint256) {
        return _decodeBatch(s.executionData).length;
    }

    function abiEdge(S calldata s) external pure returns (uint256) {
        Call[] memory calls = abi.decode(s.executionData, (Call[]));
        return calls.length;
    }

    function _decodeBatch(bytes calldata executionData)
        internal
        pure
        returns (bytes32[] calldata pointers)
    {
        assembly {
            let u := calldataload(executionData.offset)
            let s := add(executionData.offset, u)
            let e := sub(add(executionData.offset, executionData.length), 0x20)
            pointers.offset := add(s, 0x20)
            pointers.length := calldataload(s)
            if or(shr(64, u), gt(add(s, shl(5, pointers.length)), e)) {
                mstore(0x00, 0xba597e7e)
                revert(0x1c, 0x04)
            }
        }
    }
}
