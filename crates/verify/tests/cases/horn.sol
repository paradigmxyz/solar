pragma solidity ^0.8.20;
/// Loops whose invariant only the Horn-clause fallback finds: Spacer proposes it, and it is
/// then checked like every other candidate.
contract Horn {
    /// The invariant `a + b == n` relates three variables: no mined candidate gives it.
    /// @custom:ensures result == n
    function ok_transfer(uint256 n) public pure returns (uint256) {
        uint256 a = n;
        uint256 b = 0;
        while (a > 0) { a--; b++; }
        return b;
    }
    /// @custom:ensures result == n + 1
    function bad_transfer(uint256 n) public pure returns (uint256) {
        uint256 a = n;
        uint256 b = 0;
        while (a > 0) { a--; b++; }
        return b;
    }
}
