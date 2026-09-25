// `@custom:solar-view` on an `abi.decode` declaration makes its `bytes` and
// `string` variables views of the decoded bytes, so the decode must read
// `bytes` in memory or calldata, decode only value types, `bytes`, and
// `string`, and declare at least one view.
contract Test {
    struct Pair {
        uint256 a;
        bytes b;
    }

    bytes stored;

    function f(bytes memory data) public view returns (uint256 total) {
        /// @custom:solar-view
        (uint256 a, bytes memory b, string memory s) = abi.decode(data, (uint256, bytes, string));
        total = a + b.length + bytes(s).length;
        /// @custom:solar-view
        bytes memory single = abi.decode(data, (bytes));
        total += single.length;
        /// @custom:solar-view
        (uint256[] memory list, bytes memory c) = abi.decode(data, (uint256[], bytes));
        //~^ ERROR: `@custom:solar-view` cannot decode `uint256[]` in place
        total += list.length + c.length;
        /// @custom:solar-view
        (Pair memory pair) = abi.decode(data, (Pair));
        //~^ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`, or `bytes` or `string` variables initialized by `abi.decode`
        total += pair.a;
        /// @custom:solar-view
        (uint256 x, uint256 y) = abi.decode(data, (uint256, uint256));
        //~^ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`, or `bytes` or `string` variables initialized by `abi.decode`
        total += x + y;
        /// @custom:solar-view
        (bytes memory fromStorage) = abi.decode(stored, (bytes));
        //~^ ERROR: `@custom:solar-view` decodes only `bytes` held in memory or calldata
        total += fromStorage.length;
        /// @custom:solar-view
        string memory text = abi.decode(data, (string));
        total += bytes(text).length;
    }
}
