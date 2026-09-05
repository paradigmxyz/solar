//@ codegen-matrix: standard
//@ run-call: bytesLength 0x1122334455, 1, 4 => 3
//@ run-call: bytesLengthOpenEnd 0x1122334455, 2 => 3
//@ run-call: arrayLength [7, 8, 9, 10], 1, 3 => 2
//@ run-call: arrayIndex [7, 8, 9, 10], 1, 3 => 9
//@ run-call: chainedLength 0x112233445566, 1, 5 => 2
//@ run-call: stringSliceInLoop "hello", 1, 4 => 3

contract CalldataSliceLength {
    function bytesLength(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return data[start:end].length;
    }

    function bytesLengthOpenEnd(bytes calldata data, uint256 start)
        external
        pure
        returns (uint256)
    {
        return data[start:].length;
    }

    function arrayLength(uint256[] calldata a, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return a[start:end].length;
    }

    function arrayIndex(uint256[] calldata a, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return a[start:end][1];
    }

    function chainedLength(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return data[start:end][1:3].length;
    }

    function stringSliceInLoop(string calldata s, uint256 start, uint256 end)
        external
        pure
        returns (uint256 n)
    {
        // `string` slices have no `length`; count through `bytes`.
        bytes calldata b = bytes(s)[start:end];
        for (uint256 i = 0; i < b.length; i++) n++;
    }
}
