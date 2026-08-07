//@ run-call-fail: decode(bytes) 0x0000000000000000000000000000000000000000000000000000000000000020

contract AbiDecodeStorageInvalid {
    bytes private data;

    function decode(bytes memory input) external returns (uint256) {
        data = input;
        uint256[] memory decoded = abi.decode(data, (uint256[]));
        return decoded.length;
    }
}
