pragma solidity ^0.8.20;
/// One while loop with an annotated invariant: the while route closes i <= n with the words the
/// step leaves alone and the kernel checks it; the MIR invariant is discovered.
contract CountUp {
    /// @custom:ensures result == n
    function count(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        /// @custom:invariant i <= n
        while (i < n) { i++; }
        return i;
    }
}
