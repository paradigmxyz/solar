pragma solidity ^0.8.20;
/// Spec-language regressions: <==> precedence, symbolic 2 ** e in unbounded spec math.
library SpecLang {
    uint256 internal constant MASK = uint256(type(uint16).max) << 240;
    /// @custom:ensures ((w & MASK) != 0) <==> result == 1
    function ok_iff(uint256 w) internal pure returns (uint8 result) { return w & MASK != 0 ? 1 : 0; }
    /// @custom:ensures ((w & MASK) != 0) <==> result == 0
    function bad_iff(uint256 w) internal pure returns (uint8 result) { return w & MASK != 0 ? 1 : 0; }
    /// @custom:ensures result == (word / 2 ** (48 * j)) % 4294967296
    function ok_field(uint256 word, uint256 j) internal pure returns (uint256 result) { return uint32(word >> (48 * j)); }
    /// @custom:ensures result == (word / 2 ** (48 * j)) % 4294967296
    function bad_field(uint256 word, uint256 j) internal pure returns (uint256 result) { return uint32(word >> (48 * j + 1)); }
    /// @custom:ensures result == word / 2 ** j
    function ok_bigshift(uint256 word, uint256 j) internal pure returns (uint256 result) { return word >> j; }
}
