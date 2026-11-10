pragma solidity ^0.8.20;
contract Arith {
    /// @custom:ensures result == a + b
    function ok_add(uint8 a, uint8 b) public pure returns (uint8) { return a + b; }
    /// @custom:ensures result <= 255
    function ok_range(uint8 a) public pure returns (uint8) { return a; }
    /// @custom:ensures result == a + b
    function bad_unchecked(uint8 a, uint8 b) public pure returns (uint8) { unchecked { return a + b; } }
    /// @custom:ensures result == (a + b) % 256
    function ok_unchecked(uint8 a, uint8 b) public pure returns (uint8) { unchecked { return a + b; } }
    /// @custom:ensures result == -128
    function ok_wrap_signed() public pure returns (int8) { int8 x = 127; unchecked { x = x + 1; } return x; }
    /// @custom:ensures result == a / b
    function ok_div(uint256 a, uint256 b) public pure returns (uint256) { return a / b; }
    /// @custom:ensures result == -3
    function ok_sdiv() public pure returns (int256) { int256 a = -7; return a / 2; }
    /// @custom:ensures result == -1
    function ok_smod() public pure returns (int256) { int256 a = -7; return a % 2; }
    /// @custom:ensures result == uint8(x)
    function ok_cast(uint256 x) public pure returns (uint8) { return uint8(x); }
    /// @custom:ensures result == x % 256
    function ok_cast2(uint256 x) public pure returns (uint8) { return uint8(x); }
    /// @custom:ensures result == x
    function bad_cast(uint256 x) public pure returns (uint8) { return uint8(x); }
    /// @custom:ensures result == 0xff
    function ok_bitand() public pure returns (uint256) { uint256 a = 0x1ff; return a & 0xff; }
    /// @custom:ensures result == x * 8
    function ok_shl(uint16 x) public pure returns (uint256) { return uint256(x) << 3; }
    /// @custom:ensures result == type(uint256).max
    function ok_typemax() public pure returns (uint256) { return 2**256 - 1; }
    /// @custom:ensures result == 3 days
    function ok_time() public pure returns (uint256) { return 259200; }
    /// @custom:ensures result == 2 * 10**18
    function ok_ether() public pure returns (uint256) { return 2 ether; }
    function ok_short_circuit(uint256 a, uint256 b) public pure returns (bool) {
        // b / a only evaluated when a != 0: must not exclude the a == 0 path
        bool r = a != 0 && b / a > 1;
        assert(a != 0 || !r);
        return r;
    }
    function bad_short_circuit_soundness(uint256 a) public pure {
        bool r = a == 0 || 10 / a > 0;
        assert(a != 0);
    }

    function ovl(uint256 x, uint256 y) internal pure returns (uint256) { return x - y; }
    function ovl(int256 x, int256 y) internal pure returns (int256) { return x + y; }
    /// @custom:ensures result == x - y
    function ok_overload_uint(uint128 x, uint128 y) public pure returns (uint256) { return ovl(uint256(x), uint256(y)); }
    /// @custom:ensures result == x + y
    function ok_overload_int(int128 x, int128 y) public pure returns (int256) { return ovl(int256(x), int256(y)); }
    /// @custom:ensures result == x + y
    function bad_overload_uint(uint128 x, uint128 y) public pure returns (uint256) { return ovl(uint256(x), uint256(y)); }

    /// @custom:ensures result == 2 ** k
    function ok_pow2_sym(uint8 k) public pure returns (uint256) { return uint256(1) << k; }
    /// @custom:ensures result == x / (2 ** k)
    function ok_shr_sym(uint256 x, uint8 k) public pure returns (uint256) { return x >> k; }
    function ok_shl_big(uint256 x, uint256 k) public pure { require(k >= 256); assert((x << k) == 0); }
    function bad_shl_small(uint256 x, uint256 k) public pure { require(k < 256 && x > 0); assert((x << k) == 0); }
    function ok_pow_lit_base(uint8 k) public pure { require(k < 100); unchecked { assert(4 ** k == 2 ** (2 * uint256(k))); } }
}
