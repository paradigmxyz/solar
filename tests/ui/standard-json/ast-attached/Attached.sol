library Lib {
    function first(bytes calldata data) public pure returns (bytes1) {
        return data[0];
    }
}

contract Attached {
    using Lib for bytes;

    uint256[] data;
    bytes raw;

    function pops() public {
        // Neither is called.
        data.pop;
        raw.pop;
    }

    function calldataReceiver(bytes calldata input) public pure returns (bytes1) {
        return input.first();
    }
}
