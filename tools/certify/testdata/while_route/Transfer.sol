pragma solidity ^0.8.20;
/// One while loop whose invariant relates three words (a + b == n): the while route discovers
/// it with Spacer on the source loop and on the MIR loop, and the kernel checks both.
contract Transfer {
    /// @custom:ensures result == n
    function transfer(uint256 n) public pure returns (uint256) {
        uint256 a = n;
        uint256 b = 0;
        while (a > 0) { a--; b++; }
        return b;
    }
}
