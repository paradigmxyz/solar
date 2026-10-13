//@ check-pass
// `erc7201` returns a `uint256`, so dividing its slot is integer division, not a literal quotient.

contract C layout at erc7201("example.main") / 3 {
    uint256[erc7201("example.main") / erc7201("example.main") * 2 / 3 + 1] a;
}
