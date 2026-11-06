//@ codegen-matrix: standard
//@ run-call: pick 5 => 12
//@ run-call: pick 0 => 2

// The loaded value is carried into the join on the stack. The inner branch
// tests `calldatasize` itself, a word the edge must be able to materialize
// fresh: if the planned branch cannot be prepared, the copy fallback leaves
// the join without the carried word. Reduced from Solady's `ERC6551Proxy`.
contract JoinCarryRematCondition {
    uint256 stored;

    function pick(uint256 seed) external returns (uint256) {
        uint256 v = stored;
        if (v == 0) {
            v = seed + 1;
            if (msg.data.length != 0) {
                stored = v;
            }
        }
        return v * 2;
    }
}
