// `@custom:solar-view` makes the `bytes` and `string` values an `abi.decode` of calldata finds
// views of their bytes where they are. A view can only be read in place, so no use can tell it
// from the copy every other compiler makes.
contract SolarView {
    event Logged(bytes data);

    function reads(bytes calldata data)
        external
        pure
        returns (uint256, bytes32, bytes1, bytes memory, bytes memory, string memory)
    {
        /// @custom:solar-view
        (uint256 id, bytes memory payload) = abi.decode(data, (uint256, bytes));
        /// @custom:solar-view
        string memory name = abi.decode(payload, (string));
        return (
            id + payload.length + bytes(name).length,
            keccak256(payload),
            payload[0],
            abi.encode(payload, name),
            bytes.concat(payload, bytes(name)),
            string.concat(name, string(payload))
        );
    }

    function writes(bytes calldata data) external pure returns (bytes memory) {
        /// @custom:solar-view
        bytes memory v = abi.decode(data, (bytes));
        v[0] = 0x01;
        //~^ ERROR: the view `v` can only be read in place
        bytes memory w = v;
        //~^ ERROR: the view `v` can only be read in place
        v = w;
        //~^ ERROR: the view `v` can only be read in place
        delete v;
        //~^ ERROR: the view `v` can only be read in place
        assembly {
            pop(mload(v))
            //~^ ERROR: the view `v` can only be read in place
        }
        return v;
        //~^ ERROR: the view `v` can only be read in place
    }

    function passes(bytes calldata data) external {
        /// @custom:solar-view
        bytes memory v = abi.decode(data, (bytes));
        emit Logged(v);
        //~^ ERROR: the view `v` can only be read in place
        keep(v);
        //~^ ERROR: the view `v` can only be read in place
    }

    function keep(bytes memory) internal pure {}

    function shapes(bytes calldata data, bytes memory mem) external pure {
        /// @custom:solar-view
        bytes memory m = abi.decode(mem, (bytes));
        //~^ ERROR: `@custom:solar-view` decodes only `bytes` held in calldata
        /// @custom:solar-view
        uint256 x = abi.decode(data, (uint256));
        //~^ ERROR: `@custom:solar-view` declares no `bytes` or `string` variable
        /// @custom:solar-view
        (bytes memory a, uint256[] memory list) = abi.decode(data, (bytes, uint256[]));
        //~^ ERROR: `@custom:solar-view` decodes only value types, `bytes`, and `string`
        /// @custom:solar-view
        bytes memory c = mem;
        //~^ ERROR: `@custom:solar-view` requires a declaration initialized by `abi.decode`
        m; x; a; list; c;
    }
}
