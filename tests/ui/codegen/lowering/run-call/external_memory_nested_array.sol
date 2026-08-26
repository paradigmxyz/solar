//@ codegen-matrix: standard
//@ run-call: ExternalMemoryNestedArray::test() => true

contract ExternalMemoryNestedArray {
    struct Item {
        uint256 a;
        uint256 b;
        uint256 c;
        uint256 d;
        uint256 e;
    }

    function inspect(bytes[] memory values)
        external
        pure
        returns (uint256 len, bytes32 encodedHash)
    {
        return (values.length, keccak256(abi.encode(values)));
    }

    function inspectItems(Item[] calldata items) external pure returns (uint256) {
        return items[0].a + items[0].e;
    }

    function test() external view returns (bool) {
        bytes[] memory values = new bytes[](2);
        values[0] = hex"aa";
        values[1] = hex"bbcc";

        bytes32 expected = keccak256(abi.encode(values));
        (uint256 len, bytes32 actual) = this.inspect(values);

        Item[] memory items = new Item[](1);
        items[0] = Item(1, 2, 3, 4, 5);

        return len == 2 && actual == expected && this.inspectItems(items) == 6;
    }
}
