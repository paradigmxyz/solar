pragma solidity ^0.8.20;
struct Market { uint128 totalSupply; uint128 totalBorrow; uint128 lastUpdate; }
struct Range { uint128 lo; uint128 hi; }

/// A quantified invariant over a mapping of structs.
/// @custom:invariant forall bytes32 i :: market[i].totalBorrow <= market[i].totalSupply
contract Markets {
    mapping(bytes32 => Market) public market;

    /// @custom:ensures market[id].totalBorrow <= market[id].totalSupply
    function ok_loop_ptr(bytes32 id, uint256 n) external {
        Market storage m = market[id];
        /// @custom:invariant m.totalBorrow <= m.totalSupply
        /// @custom:invariant forall bytes32 j :: j != id ==> market[j] == old(market[j])
        for (uint256 i = 0; i < n; i++) {
            if (m.totalBorrow < m.totalSupply) m.totalBorrow += 1;
        }
    }
}

/// @custom:invariant range.lo <= range.hi
contract Ranges {
    Range range;

    function ok_widen(uint128 by) external { range.hi += by; }

    // expect: failing spec: range.lo <= range.hi
    // expect: counterexample (pre-state)
    function bad_invariant() external { range.lo = range.hi + 1; }
}
