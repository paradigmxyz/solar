//@ run-call: ExternalMemoryNestedArray::test() => true

contract ExternalMemoryNestedArray {
    function inspect(bytes[] memory values)
        external
        pure
        returns (uint256 len, bytes32 encodedHash)
    {
        return (values.length, keccak256(abi.encode(values)));
    }

    function test() external view returns (bool) {
        bytes[] memory values = new bytes[](2);
        values[0] = hex"aa";
        values[1] = hex"bbcc";

        bytes32 expected = keccak256(abi.encode(values));
        (uint256 len, bytes32 actual) = this.inspect(values);
        return len == 2 && actual == expected;
    }
}
