//@ run-call: use(bytes) 0x010203 => 1
//@ run-call: use(bytes) 0x01 => 1

contract CalldataSliceRecursive {
    function peel(bytes calldata data)
        internal
        pure
        returns (bytes calldata)
    {
        if (data.length < 2) return data;
        return peel(data[1:]);
    }

    function use(bytes calldata data) external pure returns (uint256) {
        return peel(data).length;
    }
}
