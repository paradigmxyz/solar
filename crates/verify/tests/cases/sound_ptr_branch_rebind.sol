pragma solidity ^0.8.20;
contract BranchPtr {
    struct S { uint256 x; }
    mapping(uint256 => S) m;

    function _get(uint256 key) internal view returns (S storage r) {
        r = m[key];
    }

    // Solidity: when c is false, p still points at m[0], so m[1].x is unchanged (0).
    /// @custom:requires m[0].x == 0 && m[1].x == 0
    /// @custom:ensures m[1].x == 5
    function bad_branch_rebind(bool c) external {
        S storage p = _get(0);
        if (c) {
            p = _get(1);
        }
        p.x = 5;
    }
    // Pointers are not merged across branches: refused rather than modelled.
    /// @custom:requires m[0].x == 0 && m[1].x == 0
    /// @custom:ensures c ? (m[1].x == 5 && m[0].x == 0) : (m[0].x == 5 && m[1].x == 0)
    // expect: storage pointer pointed at different locations on the branches
    function bad_branch_rebind_true_spec(bool c) external {
        S storage p = _get(0);
        if (c) {
            p = _get(1);
        }
        p.x = 5;
    }

    // Both branches leave p at m[0].
    /// @custom:ensures m[0].x == 5
    function ok_same_rebind(bool c) external {
        S storage p = _get(0);
        if (c) {
            p = _get(0);
        }
        p.x = 5;
    }
}
