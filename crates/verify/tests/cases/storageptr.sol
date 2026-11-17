pragma solidity ^0.8.20;
/// A storage pointer, and a storage argument written back after a call, name the location their
/// index expressions had when bound, not when used.
contract StoragePtr {
    struct S { uint256 x; }
    mapping(uint256 => S) m;
    uint256 k;

    /// @custom:ensures m[1].x == 7
    function bad_local_index() external {
        uint256 i = 0;
        S storage p = m[i];
        i = 1;
        p.x = 7;
    }
    /// @custom:ensures m[0].x == 7 && m[1].x == old(m[1].x)
    function ok_local_index() external {
        uint256 i = 0;
        S storage p = m[i];
        i = 1;
        p.x = 7;
    }
    /// @custom:ensures m[k].x == 7
    function bad_state_index() external {
        S storage p = m[k];
        k = k + 1;
        p.x = 7;
    }
    /// @custom:ensures m[k - 1].x == 7
    function ok_state_index() external {
        S storage p = m[k];
        k = k + 1;
        p.x = 7;
    }
    /// @custom:ensures m[0].x == 9
    function ok_reassigned() external {
        uint256 i = 0;
        S storage p;
        p = m[i];
        i = 1;
        p.x = 9;
    }

    function _bumpk(S storage p) internal { k = k + 1; p.x = 7; }
    /// @custom:ensures m[old(k) + 1].x == 7
    function bad_param_index() external { _bumpk(m[k]); }
    /// @custom:ensures m[old(k)].x == 7
    function ok_param_index() external { _bumpk(m[k]); }

    function _both(S storage p) internal { m[0].x = 9; p.x = p.x + 1; }
    /// @custom:ensures m[0].x == 9
    function bad_param_reaches_root() external { m[0].x = 0; _both(m[0]); }

    function _two(S storage p, S storage q) internal { p.x = 1; q.x = q.x + 5; }
    /// @custom:ensures m[0].x == 5
    function bad_param_twice() external { m[0].x = 0; _two(m[0], m[0]); }
    /// Refused although the keys differ: arguments are compared by their root.
    /// @custom:ensures m[0].x == 1 && m[1].x == 5
    function bad_param_two_keys() external { m[0].x = 0; m[1].x = 0; _two(m[0], m[1]); }

    // The callee may run outside code that reenters and changes m[0] before the write-back.
    function _poke(S storage p, address t) internal { p.x = 1; (bool ok, ) = t.call(""); require(ok); }
    // expect: may run outside code
    /// @custom:ensures m[0].x == 1
    function bad_param_reentrant(address t) external { _poke(m[0], t); }
}
