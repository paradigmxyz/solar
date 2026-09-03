contract LiteralFolds {
    function a() external pure returns (uint256) { return 2**256 - 1; }
    function b() external pure returns (int256) { return -(-2**255 + 1) - 1; }
    function d() external pure returns (int256) { return -7 % 3; }
    function e() external pure returns (int256) { return 7 % -3; }
    function f() external pure returns (uint256) { return 2 ** 3 ** 2; }
    function g() external pure returns (uint256) { return 1e18 * 1e18 / 1e36; }
    function j() external pure returns (uint256) { return (1 << 256) >> 1; }
    function k() external pure returns (uint256) { return 1 << 255 >> 255; }
    function l() external pure returns (int256) { return -1 >> 1; }
    function m() external pure returns (int256) { return -8 >> 1; }
    function n() external pure returns (uint256) { return 0 ** 1; }
    function o() external pure returns (uint256) { return 1 ** 0; }
    function p() external pure returns (uint256) { return 0 ** 0; }
    function q() external pure returns (int256) { return (-2) ** 3; }
    function r() external pure returns (int256) { return (-2) ** 2; }
    function s() external pure returns (uint256) { return 5 & 3 | 8 ^ 1; }
    function t() external pure returns (uint256) { return ~uint256(0) - 1; }
    function u() external pure returns (int256) { return ~int256(5); }
    function v() external pure returns (bool) { return 2**255 > 2**254; }
    function y() external pure returns (uint8) { return 255 + 1 - 1; }
    function z() external pure returns (int8) { return -128 - 1 + 1; }
    function aa() external pure returns (uint256) { return 2**255 * 2 / 4; }
    function ab() external pure returns (uint256) { return (2**200 + 2**200) % 7; }
    function ac() external pure returns (uint256) { return 10 ** 77 / 10 ** 76; }
    function ad() external pure returns (uint256) { return 3 ** 0 + 0 ** 3; }
    function af() external pure returns (uint256) { return 1 - 1 + 2**256 - 2**256; }
    function ah() external pure returns (uint256) { return uint256(2**255) + uint256(2**255) / 2; }
    function ai() external pure returns (uint256) { uint256 x_ = 2**255; unchecked { return x_ + 2**255; } }
    function ak() external pure returns (address) { return address(2**160 - 1); }
    function al() external pure returns (uint256) { return (2**256 - 1) % (2**128 + 1); }
    function am() external pure returns (uint256) { return 1 % 2 ** 256; }
    function an() external pure returns (uint256) { return (2 ** 256) / (2 ** 128) - 2 ** 128; }
}
