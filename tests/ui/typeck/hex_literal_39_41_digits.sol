//@ check-pass

// solc rejects hexadecimal literals of 39 and 41 digits as malformed addresses; we accept them as
// integers. See TYPECK-023 in docs/SOLC_DIVERGENCE.md.
contract C {
    uint256 a = 0x123456789012345678901234567890123456789;
    uint256 b = 0x12345678901234567890123456789012345678901;
}
