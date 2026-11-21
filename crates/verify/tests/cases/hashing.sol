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
struct Fees { uint64[4] amounts; address to; }
string constant TAG = "tag";
/// A spec's `abi.encode` hashes as the body's: fixed-size arrays expand into their elements, and a
/// string literal is one term however often it is evaluated.
contract EncodeAlike {
    /// @custom:ensures result == keccak256(abi.encode(f))
    function ok_struct_array(Fees calldata f) external pure returns (bytes32 result) {
        return keccak256(abi.encode(f));
    }
    /// @custom:ensures result == keccak256(abi.encode(xs))
    function ok_array(uint256[3] calldata xs) external pure returns (bytes32 result) {
        return keccak256(abi.encode(xs));
    }
    /// @custom:ensures result == keccak256(abi.encode(xs))
    function bad_array(uint256[3] calldata xs, uint256[3] calldata ys) external pure returns (bytes32 result) {
        return keccak256(abi.encode(ys));
    }
    /// @custom:ensures result == keccak256(abi.encode(TAG, x))
    function ok_string_constant(uint256 x) external pure returns (bytes32 result) {
        return keccak256(abi.encode(TAG, x));
    }
    /// @custom:ensures result == keccak256(abi.encode("tag", x))
    function ok_string_literal(uint256 x) external pure returns (bytes32 result) {
        return keccak256(abi.encode(TAG, x));
    }
}
/// Converting bytes to `bytesN` keeps the width: a literal's bytes sit left-aligned, and other
/// bytes read at two widths are not one term.
contract BytesWidths {
    /// @custom:ensures result
    function bad_literal_widths() external pure returns (bool) {
        return uint16(bytes2("A")) == uint16(uint8(bytes1("A")));
    }
    /// @custom:ensures result
    function ok_literal_value() external pure returns (bool) {
        return bytes2("A") == 0x4100 && bytes1("A") == 0x41;
    }
    /// @custom:ensures result
    function bad_bytes_widths(bytes memory b) external pure returns (bool) {
        return uint16(bytes2(b)) == uint16(uint8(bytes1(b)));
    }
    /// @custom:ensures result
    function ok_bytes_same_width(bytes memory b) external pure returns (bool) {
        return bytes2(b) == bytes2(b);
    }
}
/// String literals convert to `bytesN` implicitly as well, and unset bytes cast to zero.
contract ImplicitBytes {
    /// @custom:ensures result
    function bad_implicit_widths() external pure returns (bool) {
        bytes2 x = "A";
        bytes1 y = "A";
        return uint16(x) == uint16(uint8(y));
    }
    /// @custom:ensures result
    function ok_implicit_value() external pure returns (bool) {
        bytes2 x = "A";
        return x == 0x4100 && x == "A";
    }
    /// @custom:ensures r == 0
    function ok_empty_bytes() external pure returns (uint8 r) {
        bytes memory b;
        return uint8(bytes1(b));
    }
}
/// An `abi.encode` argument that writes memory changes the words of an earlier one, which the
/// compiled code reads only after every argument ran.
contract EncodeWrites {
    // expect: solver returned unknown
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(1)))
    function bad_stale_array(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, a[0] = 1));
    }
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(1)))
    function ok_no_write(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, uint256(1)));
    }
}

/// The signature's selector is all `encodeWithSignature` encodes, and distinct signatures can share it.
contract SigHashing {
    /// these two signatures share the selector 0x23b872dd, so their encodings are equal
    // expect: solver returned unknown
    /// @custom:ensures result
    function bad_sig_collision() external pure returns (bool) {
        if (keccak256(abi.encodeWithSignature("transferFrom(address,address,uint256)")) == keccak256(abi.encodeWithSignature("gasprice_bit_ether(int128)"))) {
            return keccak256(bytes("transferFrom(address,address,uint256)")) == keccak256(bytes("gasprice_bit_ether(int128)"));
        }
        return true;
    }
    /// @custom:ensures (x != y) ==> result
    function ok_sig_args(uint256 x, uint256 y) external pure returns (bool) {
        return keccak256(abi.encodeWithSignature("f(uint256)", x)) != keccak256(abi.encodeWithSignature("f(uint256)", y));
    }
}
