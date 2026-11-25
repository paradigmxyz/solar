pragma solidity ^0.8.20;
// Constant (literal) expressions with negative operands are folded as unsigned 256-bit words.
// Solidity folds literals exactly (rationals): -1 << 3 == -8, -8 & -1 == -8.
contract NegLitFold {
    int256 constant MASK = -1 << 3;
    /// @custom:ensures result > 0
    function bad_shl() public pure returns (int256) { int256 r = -1 << 3; return r; }
    /// @custom:ensures result == -8
    function ok_shl() public pure returns (int256) { int256 r = -1 << 3; return r; }
    /// @custom:ensures result > 0
    function bad_const() public pure returns (int256) { return MASK; }
    /// @custom:ensures result > 0
    function bad_and() public pure returns (int256) { int256 r = -8 & -1; return r; }
    /// @custom:ensures (a & b) >= 0
    function bad_spec_and(int8 a, int8 b) public pure {}
    /// @custom:ensures a < 0 ==> (a << 1) > 0
    function bad_spec_shl(int8 a) public pure {}
    /// @custom:ensures a < 0 && b < 0 ==> (a & b) < 0
    function ok_spec_and(int8 a, int8 b) public pure {}
    /// @custom:ensures result == -8
    function ok_and() public pure returns (int256) { int256 r = -8 & -1; return r; }
}
