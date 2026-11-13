pragma solidity ^0.8.20;
enum Color { Red, Green, Blue }
contract Base {
    uint256 internal total;
    /// @custom:ensures total == old(total) + 1
    function inc() internal { total += 1; }
}
/// @custom:invariant total >= 1
contract Storage is Base {
    mapping(address => mapping(address => uint256)) public allowance;
    uint256[] public items;
    Color public color;
    uint256 constant K = 7;

    constructor() { total = 1; }

    /// @custom:ensures allowance[msg.sender][s] == v
    /// @custom:ensures forall address o, address p :: (o != msg.sender || p != s) ==> allowance[o][p] == old(allowance[o][p])
    function ok_nested(address s, uint256 v) external { allowance[msg.sender][s] = v; }

    /// @custom:ensures items.length == old(items.length) + 1 && items[items.length - 1] == x
    function ok_push(uint256 x) external { items.push(x); }

    /// @custom:ensures items.length == old(items.length) - 1
    function ok_pop() external { items.pop(); }

    function bad_pop_strict_only() external { items.pop(); }

    /// @custom:ensures total == old(total) + 2
    function ok_modular_call() external { inc(); inc(); }

    /// @custom:ensures total == old(total) + 3
    function bad_modular_call() external { inc(); inc(); }

    function bad_break_invariant() external { total = 0; }

    /// @custom:ensures color == Color.Blue
    function ok_enum() external { color = Color.Blue; }

    /// @custom:ensures result == 14
    function ok_const() external pure returns (uint256) { return K * 2; }

    /// @custom:ensures result == x + 1
    function ok_inline(uint256 x) external pure returns (uint256) { return helper(x); }
    function helper(uint256 y) internal pure returns (uint256) { return y + 1; }

    /// @custom:ensures result == sq(x)
    function ok_specfn(uint128 x) external pure returns (uint256) { return uint256(x) * uint256(x); }
    function sq(uint256 y) internal pure returns (uint256) { return y * y; }

    /// @custom:ensures p == b && q == a
    function ok_tuple(uint256 a, uint256 b) external pure returns (uint256 p, uint256 q) {
        (uint256 x, uint256 y) = pair(b, a);
        (x, y) = (y, x);
        return (y, x);
    }
    function pair(uint256 a, uint256 b) internal pure returns (uint256, uint256) { return (a, b); }

    mapping(uint256 => uint256) public slotOf;
    modifier tagged(uint256 k) { uint256 tmp = k; require(tmp != 12345); _; }
    /// @custom:ensures slotOf[old(k)] == 1 && k == old(k)
    function ok_modifier_shadow(uint256 k) external tagged(k + 1) { slotOf[k] = 1; }
    /// @custom:ensures slotOf[old(k) + 1] == 1
    function bad_modifier_shadow(uint256 k) external tagged(k + 1) { slotOf[k] = 1; }
}
