//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// `abi.encodePacked(...)` may include a `bytes`/`string` calldata argument,
// which is packed as its raw data (no length prefix, no padding). The calldata
// is copied into a `[len][data]` memory buffer and then packed like any other
// dynamic bytes value. Used by nitro-contracts MockRollupEventInbox. The packed
// bytes (hashed here) are verified equal to solc 0.8.30 separately.

contract P {
    function h(bytes calldata a, uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, x));
    }

    function h2(bytes calldata a, address b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("pre", a, b));
    }
}
