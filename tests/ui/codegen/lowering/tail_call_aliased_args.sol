//@ compile-flags: -Ogas
//@ run-call: outer 3 => 2369
//@ run-call: other 2 => 2371

// A frame-backed tail-call argument that aliases a selected stack argument must
// be stored from a duplicate, not from the resident value's only physical copy.
contract TailCallAliasedArgs {
    function outer(uint256 x) public pure returns (uint256) {
        return mid(x);
    }

    function other(uint256 x) public pure returns (uint256) {
        return mid(x + 1) + 2;
    }

    function mid(uint256 x) internal pure returns (uint256) {
        return helper(x, x);
    }

    // The loop keeps the helper outlined, so `mid` reaches it through an
    // argument-carrying tail call with both operands aliasing `x`.
    function helper(uint256 a, uint256 b) internal pure returns (uint256) {
        unchecked {
            uint256 r = b;
            for (uint256 i = 0; i < a % 7 + 3; i++) {
                r = r * 3 + i;
            }
            return r + a;
        }
    }
}
