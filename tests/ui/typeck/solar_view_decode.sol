// `@custom:solar-view` on an `abi.decode` declaration makes its variables of
// memory reference types views of the decoded bytes, so the decode must read
// `bytes` in memory or calldata and declare at least one view. A tagged
// variable may also be initialized by a view, or by an element or a field of
// one that is a memory reference.
contract Test {
    struct Pair {
        uint256 a;
        bytes b;
    }

    bytes stored;

    function f(bytes memory data) public pure returns (uint256 total) {
        /// @custom:solar-view
        (uint256 a, bytes memory b, string memory s) = abi.decode(data, (uint256, bytes, string));
        total = a + b.length + bytes(s).length;
        /// @custom:solar-view
        bytes memory single = abi.decode(data, (bytes));
        total += single.length;
        /// @custom:solar-view
        (uint256[] memory list, bytes[] memory items, Pair memory pair) =
            abi.decode(data, (uint256[], bytes[], Pair));
        total += list.length + items.length + pair.a;
        /// @custom:solar-view
        bytes memory item = items[0];
        /// @custom:solar-view
        bytes memory payload = pair.b;
        /// @custom:solar-view
        string memory text = string(items[1]);
        total += item.length + payload.length + bytes(text).length;
    }

    function g(bytes memory data) public view returns (uint256 total) {
        /// @custom:solar-view
        (uint256[] memory list) = abi.decode(data, (uint256[]));
        /// @custom:solar-view
        uint256 first = list[0];
        //~^ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`, memory references initialized by `abi.decode`, or a memory reference read from a view
        total += first;
        bytes[] memory copies = new bytes[](1);
        /// @custom:solar-view
        bytes memory element = copies[0];
        //~^ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`, memory references initialized by `abi.decode`, or a memory reference read from a view
        total += element.length;
        /// @custom:solar-view
        (uint256 x, uint256 y) = abi.decode(data, (uint256, uint256));
        //~^ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`, memory references initialized by `abi.decode`, or a memory reference read from a view
        total += x + y;
        /// @custom:solar-view
        (bytes memory fromStorage) = abi.decode(stored, (bytes));
        //~^ ERROR: `@custom:solar-view` decodes only `bytes` held in memory or calldata
        total += fromStorage.length;
        /// @custom:solar-view
        string memory decoded = abi.decode(data, (string));
        total += bytes(decoded).length;
    }
}
