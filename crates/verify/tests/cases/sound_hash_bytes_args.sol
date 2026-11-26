pragma solidity ^0.8.20;
contract BytesHash {
    function _write(bytes memory b) private pure returns (uint256) {
        b[0] = 0x01;
        return 2;
    }
    /// @custom:ensures result
    // The hash after the write is of nothing known: refuting the claim needs a model of
    // the hash axioms, which the solver may not find in time.
    // expect: solver returned unknown
    function bad_bytes_write_in_args(bytes memory b) external pure returns (bool) {
        require(b.length > 0 && b[0] == 0x00);
        bytes32 h1 = keccak256(abi.encode(b, uint256(2)));
        bytes32 h2 = keccak256(abi.encode(b, _write(b)));
        return h1 == h2;
    }
    /// @custom:ensures result
    function ok_bytes_no_write(bytes memory b) external pure returns (bool) {
        require(b.length > 0 && b[0] == 0x00);
        bytes32 h1 = keccak256(abi.encode(b, uint256(2)));
        bytes32 h2 = keccak256(abi.encode(b, uint256(2)));
        return h1 == h2;
    }
    /// @custom:ensures result
    // expect: solver returned unknown
    function bad_bytes_index_write(bytes memory b) external pure returns (bool) {
        require(b.length > 0 && b[0] == 0x00);
        bytes32 h1 = keccak256(b);
        b[0] = 0x01;
        bytes32 h2 = keccak256(b);
        return h1 == h2;
    }
    /// @custom:ensures result
    function bad_bytes_write_packed(bytes memory b) external pure returns (bool) {
        require(b.length > 0 && b[0] == 0x00);
        bytes32 h1 = keccak256(abi.encodePacked(b, uint256(2)));
        bytes32 h2 = keccak256(abi.encodePacked(b, _write(b)));
        return h1 == h2;
    }
}
