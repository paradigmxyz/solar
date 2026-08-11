//@ compile-flags: -Ogas
//@ run-call: a 5, true => 10
//@ run-call: a 5, false => 17
//@ run-call: b 5, true => 11
//@ run-call: b 9, false => 26

// An internal argument that is live across a join and also feeds a phi on one
// arm (`r = v`) is a resident-stack-argument candidate whose value is a phi
// source on that edge. The stack scheduler must not keep it resident there: one
// physical word cannot be both the phi input and the invariant resident prefix,
// so residency is declined and the argument stays frame-passed. These calls pin
// that the join still returns the correct value.
contract ResidentArgPhiJoin {
    function a(uint256 x, bool c) external pure returns (uint256) {
        return helper(x, c);
    }

    function b(uint256 x, bool c) external pure returns (uint256) {
        return helper(x, c) + 1;
    }

    function helper(uint256 v, bool c) internal pure returns (uint256) {
        uint256 r;
        if (c) {
            r = v;
        } else {
            r = v + 7;
        }
        return r + v;
    }
}
