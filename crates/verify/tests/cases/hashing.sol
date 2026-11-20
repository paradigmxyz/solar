pragma solidity ^0.8.20;
/// Hashes are injective only over preimages: packed encodings of other widths, or with a
/// dynamic value inside, can collide, and different encodings of one value hash differently.
/// The hashes are uninterpreted, so a claim they cannot settle is unknown, not refuted.
contract Hashing {
    // expect: solver returned unknown
    /// @custom:ensures (a != c) ==> result
    function bad_packed_widths(uint8 a, uint16 b, uint16 c, uint8 d) external pure returns (bool) {
        return keccak256(abi.encodePacked(a, b)) != keccak256(abi.encodePacked(c, d));
    }
    /// @custom:ensures (a != c) ==> result
    function ok_packed_same_widths(uint8 a, uint16 b, uint8 c, uint16 d) external pure returns (bool) {
        return keccak256(abi.encodePacked(a, b)) != keccak256(abi.encodePacked(c, d));
    }
    // expect: solver returned unknown
    /// @custom:ensures (keccak256(a) != keccak256(c)) ==> result
    function bad_packed_dynamic(bytes calldata a, bytes calldata b, bytes calldata c, bytes calldata d) external pure returns (bool) {
        return keccak256(abi.encodePacked(a, b)) != keccak256(abi.encodePacked(c, d));
    }
    // expect: solver returned unknown
    /// @custom:ensures result
    function bad_sha_encodings(uint8 a) external pure returns (bool) {
        return sha256(abi.encode(a)) == sha256(abi.encodePacked(a));
    }
    // expect: solver returned unknown
    /// @custom:ensures result
    function bad_signed_packed() external pure returns (bool) {
        return keccak256(abi.encodePacked(int8(-1))) != keccak256(abi.encodePacked(uint8(255)));
    }
    /// @custom:ensures result
    function ok_same_preimage(uint8 a, uint16 b) external pure returns (bool) {
        return keccak256(abi.encodePacked(a, b)) == keccak256(abi.encodePacked(a, b));
    }
}
