pragma solidity ^0.8.20;
// keccak256 of a string literal is the real hash, a constant: a constant defined by it equals
// itself wherever it is read, and hashes built on it compare by their other arguments.
contract KeccakLiteral {
    bytes32 constant TAG = keccak256("receipt.settle.fill.v1");

    /// @custom:ensures result == TAG
    function ok_tag() external pure returns (bytes32 result) {
        result = TAG;
    }

    /// @custom:ensures result == keccak256(abi.encode(TAG, x))
    function ok_tag_encode(uint256 x) external pure returns (bytes32 result) {
        result = keccak256(abi.encode(TAG, x));
    }

    /// @custom:ensures result == keccak256(bytes("receipt.settle.fill.v1"))
    function ok_tag_bytes() external pure returns (bytes32 result) {
        result = keccak256(abi.encodePacked("receipt.settle.", "fill.v1"));
    }

    /// @custom:ensures result != TAG
    function bad_tag() external pure returns (bytes32 result) {
        result = TAG;
    }

    /// @custom:ensures result == keccak256("receipt.settle.fill.v2")
    function bad_other_tag() external pure returns (bytes32 result) {
        result = TAG;
    }
}
