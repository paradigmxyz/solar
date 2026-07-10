//@compile-flags: -Zcodegen --emit=bin-runtime

// Tuple assignment to EXISTING lvalues, `(a, b) = rhs`. `lower_assign` had no
// tuple case, so these silently assigned nothing (e.g. `(ok, ) = addr.call(d)`
// returned false regardless of the call). Handle a tuple RHS (including swaps,
// evaluating all right-hand values first), a low-level call (success flag plus
// returndata), and an ordinary multi-return call (first value plus the rest
// from memory). Runtime results verified equal to solc 0.8.30 separately.

contract C {
    function viaNamed(address t, bytes calldata d) external returns (bool ok) {
        (ok, ) = t.call(d);
    }

    function swap(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        (a, b) = (b, a);
        return (a, b);
    }

    function two() internal pure returns (uint256, uint256) {
        return (7, 9);
    }

    function multi() external pure returns (uint256 x, uint256 y) {
        x = 100;
        y = 200;
        (x, y) = two();
    }
}
