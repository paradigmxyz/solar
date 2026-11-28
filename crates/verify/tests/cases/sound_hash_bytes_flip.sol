pragma solidity ^0.8.20;
contract BytesHash2 {
    function _flip(bytes memory b) private pure returns (uint256) {
        b[0] = ~b[0];
        return 2;
    }
    /// @custom:ensures result
    // The hash after the write is of nothing known: refuting the claim needs a model of
    // the hash axioms, which the solver may not find in time.
    // expect: solver returned unknown
    function bad_bytes_flip_in_args(bytes memory b) external pure returns (bool) {
        require(b.length > 0);
        bytes32 h1 = keccak256(abi.encode(b, uint256(2)));
        bytes32 h2 = keccak256(abi.encode(b, _flip(b)));
        return h1 == h2;
    }
    /// @custom:ensures result
    function bad_string_flip_packed(string memory s) external pure returns (bool) {
        require(bytes(s).length > 0);
        bytes32 h1 = keccak256(abi.encodePacked(s, uint256(2)));
        bytes32 h2 = keccak256(abi.encodePacked(s, _flip(bytes(s))));
        return h1 == h2;
    }
    /// @custom:ensures !result
    function vacuity_probe(bytes memory b) external pure returns (bool) {
        require(b.length > 0);
        bytes32 h1 = keccak256(abi.encode(b, uint256(2)));
        bytes32 h2 = keccak256(abi.encode(b, _flip(b)));
        return h1 == h2;
    }
}
