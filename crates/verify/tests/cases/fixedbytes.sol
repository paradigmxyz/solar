pragma solidity ^0.8.20;
/// Fixed-size bytes keep their bytes in place: conversions between widths pad or cut on the
/// right, and assembly and ABI words hold them left-aligned.
contract FixedBytes {
    mapping(bytes32 => uint256) m;

    /// @custom:ensures result == 0x44
    function bad_narrow() external pure returns (bytes1) {
        bytes32 x = bytes32(uint256(0x44));
        return bytes1(x);
    }
    /// @custom:ensures result == 0
    function ok_narrow() external pure returns (bytes1) {
        bytes32 x = bytes32(uint256(0x44));
        return bytes1(x);
    }
    /// @custom:ensures result == 0x11
    function ok_narrow_lead() external pure returns (bytes1) {
        bytes4 x = 0x11223344;
        return bytes1(x);
    }
    /// @custom:ensures result == 0x44
    function bad_widen() external pure returns (bytes32) {
        bytes1 x = bytes1(uint8(0x44));
        return bytes32(x);
    }
    /// @custom:ensures uint256(result) == 0x44 * 2**248
    function ok_widen() external pure returns (bytes32) {
        bytes1 x = bytes1(uint8(0x44));
        return bytes32(x);
    }
    /// @custom:ensures uint256(result) == 0x44 * 2**248
    function ok_implicit_widen() external pure returns (bytes32 r) {
        bytes1 x = bytes1(uint8(0x44));
        r = x;
    }
    /// @custom:ensures result
    function ok_compare_widths(bytes1 x) external pure returns (bool) {
        bytes32 y = x;
        return y == x;
    }
    /// @custom:ensures result == old(m[bytes32(k)]) + 1
    function ok_key_widen(bytes4 k) external returns (uint256) {
        m[k] = m[k] + 1;
        return m[bytes32(k)];
    }
    /// @custom:ensures result == old(m[bytes32(k)])
    function bad_key_widen(bytes4 k) external returns (uint256) {
        m[k] = m[k] + 1;
        return m[bytes32(k)];
    }
    /// @custom:ensures result == 0x11223344
    function bad_asm_read() external pure returns (uint256 r) {
        bytes4 s = 0x11223344;
        assembly { r := s }
    }
    /// @custom:ensures result == 0x11223344 * 2**224
    function ok_asm_read() external pure returns (uint256 r) {
        bytes4 s = 0x11223344;
        assembly { r := s }
    }
    /// @custom:ensures result == 0x22
    function ok_index() external pure returns (bytes1) {
        bytes4 s = 0x11223344;
        return s[1];
    }
    /// @custom:ensures result == 0x44
    function bad_index() external pure returns (bytes1) {
        bytes4 s = 0x11223344;
        return s[1];
    }
    /// @custom:ensures true
    function bad_asm_write() external pure returns (bytes4 s) {
        assembly { s := 1 }
    }
}
