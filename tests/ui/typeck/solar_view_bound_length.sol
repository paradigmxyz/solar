library Mutator {
    function length(string memory text) internal pure returns (uint256) {
        bytes(text)[0] = 0x41;
        return 1;
    }
}

contract Test {
    using Mutator for string;

    function run(bytes calldata data) external pure returns (bytes1) {
        /// @custom:solar-view
        string memory text = abi.decode(data, (string));
        text.length(); //~ ERROR: the view `text` can only be read in place
        return bytes(text)[0];
    }
}
